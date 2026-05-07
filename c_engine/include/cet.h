#ifndef CET_H
#define CET_H

#include <stddef.h>
#include <stdint.h>

#define CET_MAX_SEQ 16
#define CET_MAX_EVENTS 200000
#define CET_MAX_EDGES 1000000
#define CET_MAX_PATHS 100000
#define CET_MAX_PATH_LEN 64
#define CET_MAX_GRAPHLETS 4096

typedef int (*cet_predicate_fn)(int prev_id, int curr_id, void* ctx);

typedef struct {
  char name[32];
  int kleene_plus;
  cet_predicate_fn predicate;
  void* predicate_ctx;
} cet_event_type_t;

typedef struct {
  char name[64];
  cet_event_type_t seq[CET_MAX_SEQ];
  size_t seq_len;
  int64_t within_ms;
  int64_t slide_ms;
  int skip_till_any_match;
} cet_query_t;

typedef struct { int id; char partition_key[64]; char event_type[32]; int64_t event_time_ms; } cet_vertex_t;
typedef struct { int src; int dst; int64_t window_start_ms; int64_t window_end_ms; } cet_edge_t;

typedef struct { cet_vertex_t vertices[CET_MAX_EVENTS]; cet_edge_t edges[CET_MAX_EDGES]; size_t vcount; size_t ecount; } cet_graph_t;
typedef struct { int paths[CET_MAX_PATHS][CET_MAX_PATH_LEN]; size_t path_len[CET_MAX_PATHS]; size_t count; } cet_result_t;

typedef struct { char graphlet_id[32]; int64_t start_ms; int64_t end_ms; int vertex_count; int edge_count; double memory_cost; double cpu_cost; } cet_graphlet_t;
typedef struct { int indices[CET_MAX_GRAPHLETS]; size_t count; double total_memory; double total_cpu; } cet_plan_t;
typedef struct { int shared_idx[CET_MAX_GRAPHLETS]; int new_idx[CET_MAX_GRAPHLETS]; int expired_idx[CET_MAX_GRAPHLETS]; size_t shared_count; size_t new_count; size_t expired_count; } cet_graphlet_delta_t;

typedef struct { int key_vertex; size_t seq_idx; size_t hits; } cet_partial_cache_entry_t;
typedef struct { cet_partial_cache_entry_t entries[CET_MAX_GRAPHLETS]; size_t count; } cet_partial_cache_t;

int cet_parse_query(const char* name, const char* pattern_csv, int64_t within_ms, int64_t slide_ms, cet_query_t* out);
void cet_graph_init(cet_graph_t* g);
int cet_graph_add_vertex(cet_graph_t* g, int id, const char* pkey, const char* etype, int64_t t);
int cet_graph_add_edge(cet_graph_t* g, int src, int dst, int64_t wstart, int64_t wend);

void cet_execute_mcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out);
void cet_execute_tcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out);
void cet_execute_hcet(const cet_graph_t* g, const cet_query_t* q, size_t switch_depth, cet_result_t* out);

size_t cet_materialize_windows(int64_t start, int64_t end, int64_t within, int64_t slide, int64_t out[][2], size_t cap);
void cet_estimate_costs(cet_graphlet_t* arr, size_t n);
void cet_set_cost_coefficients(double mem_vertex, double mem_edge, double cpu_edge, double cpu_vertex);
size_t cet_detect_graphlets(int64_t windows[][2], size_t nwin, cet_graphlet_t* out, size_t cap);
void cet_greedy_plan(const cet_graphlet_t* gl, size_t n, double max_mem, cet_plan_t* out);
void cet_branch_and_bound_plan(const cet_graphlet_t* gl, size_t n, double max_mem, cet_plan_t* out);
void cet_classify_graphlet_delta(const cet_graphlet_t* prev, size_t pcount, const cet_graphlet_t* curr, size_t ccount, cet_graphlet_delta_t* out);
void cet_partial_cache_init(cet_partial_cache_t* cache);
void cet_partial_cache_touch(cet_partial_cache_t* cache, int key_vertex, size_t seq_idx);

#endif
