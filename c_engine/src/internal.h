#ifndef CET_INTERNAL_H
#define CET_INTERNAL_H

#include "cet.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
  void* base;
  size_t capacity;
  size_t offset;
  int used_mmap;
} cet_runtime_workspace_t;

void cet_stats_merge(cet_exec_stats_t* dst, const cet_exec_stats_t* src);
void cet_stats_set_error(cet_exec_stats_t* stats, const char* msg);
void cet_runtime_stats_set_error(cet_runtime_stats_t* stats, const char* msg);

int cet_workspace_open(
  cet_runtime_workspace_t* ws,
  size_t bytes,
  int use_mmap,
  int madvise_hugepage,
  cet_runtime_stats_t* runtime_stats
);
void* cet_workspace_alloc(cet_runtime_workspace_t* ws, size_t bytes, size_t align);
void cet_workspace_close(cet_runtime_workspace_t* ws);

void cet_execute_tcet_range_ex(
  const cet_graph_t* g,
  const cet_query_t* q,
  size_t start_vertex_begin,
  size_t start_vertex_end,
  cet_result_t* out,
  cet_exec_stats_t* stats
);

void cet_execute_hcet_range_ex(
  const cet_graph_t* g,
  const cet_query_t* q,
  size_t switch_depth,
  size_t start_vertex_begin,
  size_t start_vertex_end,
  cet_result_t* out,
  cet_exec_stats_t* stats
);

#ifdef __cplusplus
}
#endif

#endif
