#include "cet.h"
#include "internal.h"
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

typedef struct {
  int path[CET_MAX_PATH_LEN];
  size_t len;
  size_t idx;
  int64_t start_ts;
} state_t;

typedef struct {
  int to;
  int next;
  int64_t window_start_ms;
  int64_t window_end_ms;
} adj_edge_t;

typedef struct {
  int head[CET_MAX_EVENTS];
  adj_edge_t edges[CET_MAX_EDGES];
  size_t ecount;
} adj_index_t;

void cet_exec_stats_init(cet_exec_stats_t* stats) {
  if (stats) memset(stats, 0, sizeof(*stats));
}

void cet_stats_set_error(cet_exec_stats_t* stats, const char* msg) {
  if (!stats) return;
  stats->overflow = 1;
  if (stats->error[0] == '\0' && msg) {
    snprintf(stats->error, sizeof(stats->error), "%s", msg);
  }
}

void cet_stats_merge(cet_exec_stats_t* dst, const cet_exec_stats_t* src) {
  if (!dst || !src) return;
  dst->paths_emitted += src->paths_emitted;
  dst->paths_truncated += src->paths_truncated;
  dst->states_enqueued += src->states_enqueued;
  dst->states_truncated += src->states_truncated;
  dst->seed_paths += src->seed_paths;
  if (src->max_depth_seen > dst->max_depth_seen) dst->max_depth_seen = src->max_depth_seen;
  dst->temporal_rejects += src->temporal_rejects;
  dst->edge_window_rejects += src->edge_window_rejects;
  dst->predicate_rejects += src->predicate_rejects;
  if (src->overflow) dst->overflow = 1;
  if (dst->error[0] == '\0' && src->error[0] != '\0') {
    snprintf(dst->error, sizeof(dst->error), "%s", src->error);
  }
}

static void note_depth(cet_exec_stats_t* stats, size_t depth) {
  if (stats && depth > stats->max_depth_seen) stats->max_depth_seen = depth;
}

static const cet_vertex_t* find_v(const cet_graph_t* g, int id) {
  for (size_t i = 0; i < g->vcount; i++) {
    if (g->vertices[i].id == id) return &g->vertices[i];
  }
  return NULL;
}

static int vpos(const cet_graph_t* g, int id) {
  for (size_t i = 0; i < g->vcount; i++) {
    if (g->vertices[i].id == id) return (int)i;
  }
  return -1;
}

static void emit_ex(cet_result_t* out, const int* path, size_t len, cet_exec_stats_t* stats) {
  note_depth(stats, len);
  if (len > CET_MAX_PATH_LEN) {
    if (stats) stats->paths_truncated++;
    cet_stats_set_error(stats, "path length exceeded CET_MAX_PATH_LEN");
    return;
  }
  if (out->count >= CET_MAX_PATHS) {
    if (stats) stats->paths_truncated++;
    cet_stats_set_error(stats, "result count exceeded CET_MAX_PATHS");
    return;
  }
  memcpy(out->paths[out->count], path, len * sizeof(int));
  out->path_len[out->count] = len;
  out->count++;
  if (stats) stats->paths_emitted++;
}

static int edge_temporal_ok(
  const cet_graph_t* g,
  const cet_query_t* q,
  int prev_id,
  int curr_id,
  int64_t start_time,
  const adj_edge_t* e,
  cet_exec_stats_t* stats
) {
  const cet_vertex_t* pv = find_v(g, prev_id);
  const cet_vertex_t* nv = find_v(g, curr_id);
  if (!pv || !nv) {
    if (stats) stats->temporal_rejects++;
    return 0;
  }

  /* Strong event-time monotonicity: CET paths are causal, not merely connected. */
  if (nv->event_time_ms < start_time || nv->event_time_ms < pv->event_time_ms) {
    if (stats) stats->temporal_rejects++;
    return 0;
  }

  if (q->within_ms >= 0 && (nv->event_time_ms - start_time) > q->within_ms) {
    if (stats) stats->temporal_rejects++;
    return 0;
  }

  /* Edge windows are optional for older callers. If present, validate both endpoints. */
  if (e && e->window_end_ms > e->window_start_ms) {
    if (pv->event_time_ms < e->window_start_ms ||
        pv->event_time_ms > e->window_end_ms ||
        nv->event_time_ms < e->window_start_ms ||
        nv->event_time_ms > e->window_end_ms) {
      if (stats) stats->edge_window_rejects++;
      return 0;
    }
  }

  return 1;
}

static int type_and_pred_match(
  const cet_graph_t* g,
  const cet_query_t* q,
  int prev_id,
  int curr_id,
  size_t idx,
  cet_exec_stats_t* stats
) {
  const cet_vertex_t* cv = find_v(g, curr_id);
  if (!cv || idx >= q->seq_len) {
    if (stats) stats->predicate_rejects++;
    return 0;
  }
  if (strcmp(cv->event_type, q->seq[idx].name) != 0) {
    if (stats) stats->predicate_rejects++;
    return 0;
  }
  if (q->seq[idx].predicate && !q->seq[idx].predicate(prev_id, curr_id, q->seq[idx].predicate_ctx)) {
    if (stats) stats->predicate_rejects++;
    return 0;
  }
  return 1;
}

static void build_adj(const cet_graph_t* g, adj_index_t* a) {
  for (size_t i = 0; i < g->vcount; i++) a->head[i] = -1;
  a->ecount = 0;
  for (size_t i = 0; i < g->ecount && a->ecount < CET_MAX_EDGES; i++) {
    int sp = vpos(g, g->edges[i].src);
    if (sp < 0) continue;
    a->edges[a->ecount].to = g->edges[i].dst;
    a->edges[a->ecount].window_start_ms = g->edges[i].window_start_ms;
    a->edges[a->ecount].window_end_ms = g->edges[i].window_end_ms;
    a->edges[a->ecount].next = a->head[sp];
    a->head[sp] = (int)a->ecount;
    a->ecount++;
  }
}

static void dfs(
  const cet_graph_t* g,
  const cet_query_t* q,
  const adj_index_t* a,
  int* path,
  size_t len,
  size_t idx,
  int64_t start_time,
  cet_result_t* out,
  cet_exec_stats_t* stats
) {
  note_depth(stats, len);
  if (len >= CET_MAX_PATH_LEN) {
    if (stats) stats->paths_truncated++;
    cet_stats_set_error(stats, "DFS path length reached CET_MAX_PATH_LEN");
    return;
  }
  if (idx >= q->seq_len) {
    emit_ex(out, path, len, stats);
    return;
  }

  int prev = path[len - 1];
  int p = vpos(g, prev);
  if (p < 0) return;

  for (int ei = a->head[p]; ei != -1; ei = a->edges[ei].next) {
    const adj_edge_t* ae = &a->edges[ei];
    int nxt = ae->to;

    if (!edge_temporal_ok(g, q, prev, nxt, start_time, ae, stats)) continue;

    if (type_and_pred_match(g, q, prev, nxt, idx, stats)) {
      path[len] = nxt;
      if (q->seq[idx].kleene_plus) {
        dfs(g, q, a, path, len + 1, idx, start_time, out, stats);
      }
      dfs(g, q, a, path, len + 1, idx + 1, start_time, out, stats);
    } else if (q->skip_till_any_match) {
      path[len] = nxt;
      dfs(g, q, a, path, len + 1, idx, start_time, out, stats);
    }
  }
}

static int enqueue_state(state_t* qbuf, size_t* tail, const state_t* s, cet_exec_stats_t* stats) {
  if (*tail >= CET_MAX_PATHS) {
    if (stats) stats->states_truncated++;
    cet_stats_set_error(stats, "BFS state queue exceeded CET_MAX_PATHS");
    return 0;
  }
  qbuf[*tail] = *s;
  (*tail)++;
  if (stats) stats->states_enqueued++;
  note_depth(stats, s->len);
  return 1;
}

void cet_execute_mcet_ex(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out, cet_exec_stats_t* stats) {
  memset(out, 0, sizeof(*out));
  cet_exec_stats_init(stats);
  if (!g || !q || q->seq_len == 0) return;

  adj_index_t* a = (adj_index_t*)calloc(1, sizeof(adj_index_t));
  if (!a) {
    cet_stats_set_error(stats, "failed to allocate adjacency index");
    return;
  }

  build_adj(g, a);
  int path[CET_MAX_PATH_LEN];

  for (size_t i = 0; i < g->vcount; i++) {
    if (strcmp(g->vertices[i].event_type, q->seq[0].name) == 0) {
      path[0] = g->vertices[i].id;
      dfs(g, q, a, path, 1, 1, g->vertices[i].event_time_ms, out, stats);
    }
  }

  free(a);
}

void cet_execute_tcet_range_ex(
  const cet_graph_t* g,
  const cet_query_t* q,
  size_t start_vertex_begin,
  size_t start_vertex_end,
  cet_result_t* out,
  cet_exec_stats_t* stats
) {
  memset(out, 0, sizeof(*out));
  cet_exec_stats_init(stats);
  if (!g || !q || q->seq_len == 0) return;
  if (start_vertex_begin > g->vcount) start_vertex_begin = g->vcount;
  if (start_vertex_end > g->vcount) start_vertex_end = g->vcount;
  if (start_vertex_end < start_vertex_begin) start_vertex_end = start_vertex_begin;

  state_t* qbuf = (state_t*)calloc(CET_MAX_PATHS, sizeof(state_t));
  if (!qbuf) {
    cet_stats_set_error(stats, "failed to allocate BFS state queue");
    return;
  }

  adj_index_t* a = (adj_index_t*)calloc(1, sizeof(adj_index_t));
  if (!a) {
    free(qbuf);
    cet_stats_set_error(stats, "failed to allocate adjacency index");
    return;
  }

  build_adj(g, a);

  size_t head = 0, tail = 0;
  cet_partial_cache_t cache;
  cet_partial_cache_init(&cache);

  for (size_t i = start_vertex_begin; i < start_vertex_end; i++) {
    if (strcmp(g->vertices[i].event_type, q->seq[0].name) == 0) {
      state_t s;
      memset(&s, 0, sizeof(s));
      s.path[0] = g->vertices[i].id;
      s.len = 1;
      s.idx = 1;
      s.start_ts = g->vertices[i].event_time_ms;
      if (!enqueue_state(qbuf, &tail, &s, stats)) break;
    }
  }

  while (head < tail) {
    state_t s = qbuf[head++];

    if (s.idx >= q->seq_len) {
      emit_ex(out, s.path, s.len, stats);
      continue;
    }

    int last = s.path[s.len - 1];
    int p = vpos(g, last);
    if (p < 0) continue;

    for (int ei = a->head[p]; ei != -1; ei = a->edges[ei].next) {
      const adj_edge_t* ae = &a->edges[ei];
      int nxt = ae->to;

      if (!edge_temporal_ok(g, q, last, nxt, s.start_ts, ae, stats)) continue;

      if (type_and_pred_match(g, q, last, nxt, s.idx, stats)) {
        state_t ns = s;
        if (ns.len >= CET_MAX_PATH_LEN) {
          if (stats) stats->states_truncated++;
          cet_stats_set_error(stats, "BFS path length reached CET_MAX_PATH_LEN");
          continue;
        }
        ns.path[ns.len++] = nxt;
        cet_partial_cache_touch(&cache, nxt, s.idx);

        if (q->seq[s.idx].kleene_plus) {
          state_t ks = ns;
          ks.idx = s.idx;
          enqueue_state(qbuf, &tail, &ks, stats);
        }

        ns.idx = s.idx + 1;
        enqueue_state(qbuf, &tail, &ns, stats);
      } else if (q->skip_till_any_match) {
        state_t ns = s;
        if (ns.len >= CET_MAX_PATH_LEN) {
          if (stats) stats->states_truncated++;
          cet_stats_set_error(stats, "BFS path length reached CET_MAX_PATH_LEN");
          continue;
        }
        ns.path[ns.len++] = nxt;
        enqueue_state(qbuf, &tail, &ns, stats);
      }
    }
  }

  free(a);
  free(qbuf);
}

void cet_execute_tcet_ex(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out, cet_exec_stats_t* stats) {
  cet_execute_tcet_range_ex(g, q, 0, g ? g->vcount : 0, out, stats);
}

void cet_execute_hcet_range_ex(
  const cet_graph_t* g,
  const cet_query_t* q,
  size_t switch_depth,
  size_t start_vertex_begin,
  size_t start_vertex_end,
  cet_result_t* out,
  cet_exec_stats_t* stats
) {
  if (switch_depth <= 1) {
    cet_execute_tcet_range_ex(g, q, start_vertex_begin, start_vertex_end, out, stats);
    return;
  }

  memset(out, 0, sizeof(*out));
  cet_exec_stats_init(stats);
  if (!g || !q || q->seq_len == 0) return;

  cet_query_t prefix = *q;
  if (prefix.seq_len > switch_depth) prefix.seq_len = switch_depth;

  cet_result_t* seeds = (cet_result_t*)calloc(1, sizeof(cet_result_t));
  if (!seeds) {
    cet_stats_set_error(stats, "failed to allocate H-CET seed result");
    return;
  }

  cet_exec_stats_t prefix_stats;
  cet_execute_tcet_range_ex(g, &prefix, start_vertex_begin, start_vertex_end, seeds, &prefix_stats);

  if (stats) {
    *stats = prefix_stats;
    stats->seed_paths = seeds->count;
    stats->paths_emitted = 0;
  }

  adj_index_t* a = (adj_index_t*)calloc(1, sizeof(adj_index_t));
  if (!a) {
    free(seeds);
    cet_stats_set_error(stats, "failed to allocate adjacency index");
    return;
  }

  build_adj(g, a);

  for (size_t i = 0; i < seeds->count; i++) {
    int path[CET_MAX_PATH_LEN];
    size_t len = seeds->path_len[i];

    if (len > CET_MAX_PATH_LEN) {
      if (stats) stats->paths_truncated++;
      cet_stats_set_error(stats, "seed path exceeded CET_MAX_PATH_LEN");
      continue;
    }

    memcpy(path, seeds->paths[i], len * sizeof(int));

    if (prefix.seq_len >= q->seq_len) {
      emit_ex(out, path, len, stats);
      continue;
    }

    const cet_vertex_t* start = find_v(g, path[0]);
    dfs(g, q, a, path, len, prefix.seq_len, start ? start->event_time_ms : 0, out, stats);
  }

  free(a);
  free(seeds);
}

void cet_execute_hcet_ex(const cet_graph_t* g, const cet_query_t* q, size_t switch_depth, cet_result_t* out, cet_exec_stats_t* stats) {
  cet_execute_hcet_range_ex(g, q, switch_depth, 0, g ? g->vcount : 0, out, stats);
}

void cet_execute_mcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out) {
  cet_exec_stats_t stats;
  cet_execute_mcet_ex(g, q, out, &stats);
}

void cet_execute_tcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out) {
  cet_exec_stats_t stats;
  cet_execute_tcet_ex(g, q, out, &stats);
}

void cet_execute_hcet(const cet_graph_t* g, const cet_query_t* q, size_t switch_depth, cet_result_t* out) {
  cet_exec_stats_t stats;
  cet_execute_hcet_ex(g, q, switch_depth, out, &stats);
}
