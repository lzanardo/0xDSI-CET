#include "cet.h"
#include <string.h>
#include <stdlib.h>

typedef struct { int path[CET_MAX_PATH_LEN]; size_t len; size_t idx; int64_t start_ts; } state_t;

typedef struct { int to; int next; } adj_edge_t;

typedef struct {
  int head[CET_MAX_EVENTS];
  adj_edge_t edges[CET_MAX_EDGES];
  size_t ecount;
} adj_index_t;

static const cet_vertex_t* find_v(const cet_graph_t* g, int id) { for (size_t i=0;i<g->vcount;i++) if (g->vertices[i].id==id) return &g->vertices[i]; return NULL; }
static int vpos(const cet_graph_t* g, int id) { for (size_t i=0;i<g->vcount;i++) if (g->vertices[i].id==id) return (int)i; return -1; }
static void emit(cet_result_t* out, const int* path, size_t len){ if(out->count>=CET_MAX_PATHS || len>CET_MAX_PATH_LEN) return; memcpy(out->paths[out->count], path, len*sizeof(int)); out->path_len[out->count]=len; out->count++; }
static int within(const cet_vertex_t* v, int64_t start, const cet_query_t* q){ return v && (v->event_time_ms-start)<=q->within_ms; }
static int type_and_pred_match(const cet_graph_t* g, const cet_query_t* q, int prev_id, int curr_id, size_t idx){ const cet_vertex_t* cv=find_v(g,curr_id); if(!cv) return 0; if(strcmp(cv->event_type,q->seq[idx].name)!=0) return 0; if(q->seq[idx].predicate && !q->seq[idx].predicate(prev_id,curr_id,q->seq[idx].predicate_ctx)) return 0; return 1; }

static void build_adj(const cet_graph_t* g, adj_index_t* a){
  for(size_t i=0;i<g->vcount;i++) a->head[i] = -1;
  a->ecount = 0;
  for(size_t i=0;i<g->ecount && a->ecount<CET_MAX_EDGES;i++){
    int sp = vpos(g, g->edges[i].src); if(sp<0) continue;
    a->edges[a->ecount].to = g->edges[i].dst;
    a->edges[a->ecount].next = a->head[sp];
    a->head[sp] = (int)a->ecount;
    a->ecount++;
  }
}

static void dfs(const cet_graph_t* g, const cet_query_t* q, const adj_index_t* a, int* path, size_t len, size_t idx, int64_t start_time, cet_result_t* out){
  if(len>=CET_MAX_PATH_LEN) return;
  if(idx>=q->seq_len){ emit(out,path,len); return; }
  int prev=path[len-1];
  int p = vpos(g, prev); if(p<0) return;
  for(int ei=a->head[p]; ei!=-1; ei=a->edges[ei].next){
    int nxt=a->edges[ei].to; const cet_vertex_t* nv=find_v(g,nxt); if(!within(nv,start_time,q)) continue;
    if(type_and_pred_match(g,q,prev,nxt,idx)){
      path[len]=nxt;
      if(q->seq[idx].kleene_plus) dfs(g,q,a,path,len+1,idx,start_time,out);
      dfs(g,q,a,path,len+1,idx+1,start_time,out);
    } else if(q->skip_till_any_match){
      path[len]=nxt;
      dfs(g,q,a,path,len+1,idx,start_time,out);
    }
  }
}

void cet_execute_mcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out){
  memset(out,0,sizeof(*out));
  adj_index_t* a=(adj_index_t*)calloc(1,sizeof(adj_index_t)); if(!a) return;
  build_adj(g,a);
  int path[CET_MAX_PATH_LEN];
  for(size_t i=0;i<g->vcount;i++) if(strcmp(g->vertices[i].event_type,q->seq[0].name)==0){ path[0]=g->vertices[i].id; dfs(g,q,a,path,1,1,g->vertices[i].event_time_ms,out);} 
  free(a);
}

void cet_execute_tcet(const cet_graph_t* g, const cet_query_t* q, cet_result_t* out){
  memset(out,0,sizeof(*out));
  state_t* qbuf=(state_t*)calloc(CET_MAX_PATHS,sizeof(state_t)); if(!qbuf) return;
  adj_index_t* a=(adj_index_t*)calloc(1,sizeof(adj_index_t)); if(!a){ free(qbuf); return; }
  build_adj(g,a);
  size_t head=0,tail=0;
  cet_partial_cache_t cache; cet_partial_cache_init(&cache);
  for(size_t i=0;i<g->vcount && tail<CET_MAX_PATHS;i++) if(strcmp(g->vertices[i].event_type,q->seq[0].name)==0){ qbuf[tail].path[0]=g->vertices[i].id; qbuf[tail].len=1; qbuf[tail].idx=1; qbuf[tail].start_ts=g->vertices[i].event_time_ms; tail++; }
  while(head<tail){
    state_t s=qbuf[head++]; if(s.idx>=q->seq_len){ emit(out,s.path,s.len); continue; }
    int last=s.path[s.len-1]; int p=vpos(g,last); if(p<0) continue;
    for(int ei=a->head[p]; ei!=-1 && tail<CET_MAX_PATHS; ei=a->edges[ei].next){
      int nxt=a->edges[ei].to; const cet_vertex_t* nv=find_v(g,nxt); if(!within(nv,s.start_ts,q)) continue;
      if(type_and_pred_match(g,q,last,nxt,s.idx)){
        state_t ns=s; if(ns.len>=CET_MAX_PATH_LEN) continue; ns.path[ns.len++]=nxt;
        cet_partial_cache_touch(&cache, nxt, s.idx);
        if(q->seq[s.idx].kleene_plus && tail<CET_MAX_PATHS){ ns.idx=s.idx; qbuf[tail++]=ns; }
        ns.idx=s.idx+1; if(tail<CET_MAX_PATHS) qbuf[tail++]=ns;
      } else if(q->skip_till_any_match){
        state_t ns=s; if(ns.len>=CET_MAX_PATH_LEN) continue; ns.path[ns.len++]=nxt;
        if(tail<CET_MAX_PATHS) qbuf[tail++]=ns;
      }
    }
  }
  free(a); free(qbuf);
}

void cet_execute_hcet(const cet_graph_t* g, const cet_query_t* q, size_t switch_depth, cet_result_t* out){
  if(switch_depth<=1){ cet_execute_tcet(g,q,out); return; }
  cet_query_t prefix=*q; if(prefix.seq_len>switch_depth) prefix.seq_len=switch_depth;
  cet_result_t* seeds=(cet_result_t*)calloc(1,sizeof(cet_result_t)); if(!seeds){ memset(out,0,sizeof(*out)); return; }
  cet_execute_tcet(g,&prefix,seeds); memset(out,0,sizeof(*out));
  adj_index_t* a=(adj_index_t*)calloc(1,sizeof(adj_index_t)); if(!a){ free(seeds); return; }
  build_adj(g,a);
  for(size_t i=0;i<seeds->count;i++){
    int path[CET_MAX_PATH_LEN]; memcpy(path,seeds->paths[i],seeds->path_len[i]*sizeof(int)); size_t len=seeds->path_len[i];
    if(prefix.seq_len>=q->seq_len){ emit(out,path,len); continue; }
    const cet_vertex_t* start=find_v(g,path[0]); dfs(g,q,a,path,len,prefix.seq_len,start?start->event_time_ms:0,out);
  }
  free(a); free(seeds);
}
