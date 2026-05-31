#include "cet.h"
#include "internal.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#if defined(CET_ENABLE_PTHREADS)
#include <pthread.h>
#endif

typedef struct {
  const cet_graph_t* g;
  const cet_query_t* q;
  size_t switch_depth;
  size_t begin;
  size_t end;
  cet_result_t* out;
  cet_exec_stats_t stats;
  int rc;
} cet_worker_ctx_t;

static size_t normalize_threads(const cet_graph_t* g, const cet_runtime_config_t* cfg, cet_runtime_stats_t* rt) {
  size_t requested = cfg ? cfg->native_threads : 1;
  if (requested == 0) requested = 1;
  if (requested > CET_MAX_NATIVE_THREADS) requested = CET_MAX_NATIVE_THREADS;
  if (g && g->vcount > 0 && requested > g->vcount) requested = g->vcount;
  if (requested == 0) requested = 1;
  if (rt) rt->native_threads_requested = cfg ? cfg->native_threads : 1;
  return requested;
}

static void run_worker(cet_worker_ctx_t* ctx) {
  if (!ctx) return;
  cet_execute_hcet_range_ex(
    ctx->g,
    ctx->q,
    ctx->switch_depth,
    ctx->begin,
    ctx->end,
    ctx->out,
    &ctx->stats
  );
  ctx->rc = 0;
}

#if defined(CET_ENABLE_PTHREADS)
static void* run_worker_thread(void* arg) {
  run_worker((cet_worker_ctx_t*)arg);
  return NULL;
}
#endif

static int merge_worker_results(cet_worker_ctx_t* workers, size_t n, cet_result_t* out, cet_exec_stats_t* stats) {
  memset(out, 0, sizeof(*out));
  cet_exec_stats_init(stats);

  for (size_t w = 0; w < n; w++) {
    cet_stats_merge(stats, &workers[w].stats);
    if (!workers[w].out) continue;

    for (size_t i = 0; i < workers[w].out->count; i++) {
      size_t len = workers[w].out->path_len[i];
      if (out->count >= CET_MAX_PATHS || len > CET_MAX_PATH_LEN) {
        if (stats) stats->paths_truncated++;
        cet_stats_set_error(stats, "parallel merge exceeded result capacity");
        continue;
      }
      memcpy(out->paths[out->count], workers[w].out->paths[i], len * sizeof(int));
      out->path_len[out->count] = len;
      out->count++;
    }
  }
  if (stats) stats->paths_emitted = out->count;
  return 0;
}

int cet_execute_hcet_parallel_ex(
  const cet_graph_t* g,
  const cet_query_t* q,
  size_t switch_depth,
  const cet_runtime_config_t* cfg,
  cet_result_t* out,
  cet_exec_stats_t* stats,
  cet_runtime_stats_t* runtime_stats
) {
  if (!out) return -1;
  memset(out, 0, sizeof(*out));
  cet_exec_stats_init(stats);
  cet_runtime_stats_init(runtime_stats);

  cet_runtime_config_t local_cfg;
  if (!cfg) {
    cet_runtime_config_default(&local_cfg);
    cfg = &local_cfg;
  }

  if (runtime_stats) {
    runtime_stats->native_threads_requested = cfg->native_threads;
  }

  if (!g || !q || q->seq_len == 0) {
    cet_runtime_stats_set_error(runtime_stats, "invalid graph or query");
    return -1;
  }

  size_t workers_n = normalize_threads(g, cfg, runtime_stats);

#if !defined(CET_ENABLE_PTHREADS)
  if (workers_n > 1) {
    cet_runtime_stats_set_error(runtime_stats, "CET_ENABLE_PTHREADS is disabled; falling back to single-thread execution");
  }
  workers_n = 1;
#endif

  if (workers_n <= 1) {
    if (runtime_stats) {
      runtime_stats->native_threads_used = 1;
      runtime_stats->parallel_enabled = 0;
    }
    cet_execute_hcet_ex(g, q, switch_depth, out, stats);
    return 0;
  }

  size_t workspace_bytes =
    workers_n * sizeof(cet_worker_ctx_t) +
    workers_n * sizeof(cet_result_t)
#if defined(CET_ENABLE_PTHREADS)
    + workers_n * sizeof(pthread_t)
    + workers_n * sizeof(int)
#endif
    + 4096;

  if (cfg->mmap_workspace_bytes > workspace_bytes) {
    workspace_bytes = cfg->mmap_workspace_bytes;
  }

  cet_runtime_workspace_t ws;
  if (cet_workspace_open(&ws, workspace_bytes, cfg->enable_mmap_arena, cfg->madvise_hugepage, runtime_stats) != 0) {
    cet_execute_hcet_ex(g, q, switch_depth, out, stats);
    return 0;
  }

  cet_worker_ctx_t* workers = (cet_worker_ctx_t*)cet_workspace_alloc(&ws, workers_n * sizeof(cet_worker_ctx_t), sizeof(void*));
  cet_result_t* results = (cet_result_t*)cet_workspace_alloc(&ws, workers_n * sizeof(cet_result_t), sizeof(void*));
#if defined(CET_ENABLE_PTHREADS)
  pthread_t* tids = (pthread_t*)cet_workspace_alloc(&ws, workers_n * sizeof(pthread_t), sizeof(void*));
  int* launched = (int*)cet_workspace_alloc(&ws, workers_n * sizeof(int), sizeof(int));
#endif

  if (!workers || !results
#if defined(CET_ENABLE_PTHREADS)
      || !tids || !launched
#endif
  ) {
    cet_runtime_stats_set_error(runtime_stats, "parallel workspace too small; falling back to single-thread execution");
    cet_workspace_close(&ws);
    cet_execute_hcet_ex(g, q, switch_depth, out, stats);
    return 0;
  }

  size_t base = g->vcount / workers_n;
  size_t rem = g->vcount % workers_n;
  size_t cursor = 0;
  for (size_t i = 0; i < workers_n; i++) {
    size_t span = base + (i < rem ? 1 : 0);
    workers[i].g = g;
    workers[i].q = q;
    workers[i].switch_depth = switch_depth;
    workers[i].begin = cursor;
    workers[i].end = cursor + span;
    workers[i].out = &results[i];
    workers[i].rc = -1;
    cursor += span;
  }

#if defined(CET_ENABLE_PTHREADS)
  for (size_t i = 0; i < workers_n; i++) {
    int rc = pthread_create(&tids[i], NULL, run_worker_thread, &workers[i]);
    if (rc != 0) {
      launched[i] = 0;
      cet_runtime_stats_set_error(runtime_stats, "pthread_create failed; executing shard on caller thread");
      run_worker(&workers[i]);
    } else {
      launched[i] = 1;
    }
  }
  for (size_t i = 0; i < workers_n; i++) {
    if (launched[i]) (void)pthread_join(tids[i], NULL);
  }
#else
  for (size_t i = 0; i < workers_n; i++) run_worker(&workers[i]);
#endif

  merge_worker_results(workers, workers_n, out, stats);

  if (runtime_stats) {
    runtime_stats->native_threads_used = workers_n;
    runtime_stats->parallel_enabled = 1;
    runtime_stats->workspace_bytes = ws.capacity;
    runtime_stats->used_mmap_arena = ws.used_mmap;
  }

  cet_workspace_close(&ws);
  return 0;
}
