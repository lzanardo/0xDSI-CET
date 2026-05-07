#include "cet.h"
#include <assert.h>
#include <string.h>
#include <stdlib.h>

int main(void){
  cet_query_t q;
  assert(cet_parse_query("q","A+,B,C",60000,10000,&q)==0);

  cet_graph_t* g = (cet_graph_t*)calloc(1,sizeof(cet_graph_t));
  assert(g); cet_graph_init(g);
  cet_graph_add_vertex(g,1,"p","A",1);
  cet_graph_add_vertex(g,2,"p","X",2);
  cet_graph_add_vertex(g,3,"p","A",3);
  cet_graph_add_vertex(g,4,"p","B",4);
  cet_graph_add_vertex(g,5,"p","C",5);
  cet_graph_add_edge(g,1,2,0,100);
  cet_graph_add_edge(g,2,3,0,100);
  cet_graph_add_edge(g,3,4,0,100);
  cet_graph_add_edge(g,4,5,0,100);

  cet_result_t* m=(cet_result_t*)calloc(1,sizeof(cet_result_t));
  cet_result_t* t=(cet_result_t*)calloc(1,sizeof(cet_result_t));
  cet_result_t* h=(cet_result_t*)calloc(1,sizeof(cet_result_t));
  assert(m&&t&&h);
  cet_execute_mcet(g,&q,m);
  cet_execute_tcet(g,&q,t);
  cet_execute_hcet(g,&q,2,h);
  assert(m->count>0 && t->count>0 && h->count>0);

  int64_t w[16][2];
  size_t wn = cet_materialize_windows(0,100,20,10,w,16);
  assert(wn>0 && w[0][0]==0 && w[0][1]==20);

  cet_graphlet_t prev[16], curr[16];
  size_t p = cet_detect_graphlets(w, wn-1, prev, 16);
  size_t c = cet_detect_graphlets(w+1, wn-1, curr, 16);
  cet_graphlet_delta_t delta; cet_classify_graphlet_delta(prev,p,curr,c,&delta);
  assert(delta.shared_count>0 && delta.new_count>0 && delta.expired_count>0);

  cet_plan_t gp,bp;
  cet_greedy_plan(curr,c,1000.0,&gp);
  cet_branch_and_bound_plan(curr,c,1000.0,&bp);
  assert(c>0 && gp.count>0 && bp.count>0);

  cet_partial_cache_t cache; cet_partial_cache_init(&cache);
  cet_partial_cache_touch(&cache,4,2); cet_partial_cache_touch(&cache,4,2);
  assert(cache.count==1 && cache.entries[0].hits==2);

  free(m); free(t); free(h); free(g);
  return 0;
}
