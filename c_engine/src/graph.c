#include "cet.h"
#include <string.h>
#include <stdio.h>

void cet_graph_init(cet_graph_t* g) { memset(g, 0, sizeof(*g)); }

int cet_graph_add_vertex(cet_graph_t* g, int id, const char* pkey, const char* etype, int64_t t) {
  if (g->vcount >= CET_MAX_EVENTS) return -1;
  cet_vertex_t* v = &g->vertices[g->vcount++];
  v->id = id; v->event_time_ms = t;
  snprintf(v->partition_key, sizeof(v->partition_key), "%s", pkey);
  snprintf(v->event_type, sizeof(v->event_type), "%s", etype);
  return 0;
}

int cet_graph_add_edge(cet_graph_t* g, int src, int dst, int64_t wstart, int64_t wend) {
  if (g->ecount >= CET_MAX_EDGES) return -1;
  cet_edge_t* e = &g->edges[g->ecount++];
  e->src = src; e->dst = dst; e->window_start_ms = wstart; e->window_end_ms = wend;
  return 0;
}
