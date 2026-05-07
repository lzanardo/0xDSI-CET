cmake_minimum_required(VERSION 3.16)
project(oxdsi_cet C)
set(CMAKE_C_STANDARD 11)

set(CET_SOURCES
  c_engine/src/dsl.c
  c_engine/src/graph.c
  c_engine/src/algorithms.c
  c_engine/src/optimizer.c
  c_engine/src/sliding.c
)

add_library(oxdsi_cet_static STATIC ${CET_SOURCES})
add_library(oxdsi_cet SHARED ${CET_SOURCES})
set_target_properties(oxdsi_cet PROPERTIES OUTPUT_NAME oxdsi_cet)

target_include_directories(oxdsi_cet_static PUBLIC c_engine/include)
target_include_directories(oxdsi_cet PUBLIC c_engine/include)

add_executable(test_cet c_engine/tests/test_main.c)
target_link_libraries(test_cet PRIVATE oxdsi_cet_static)

enable_testing()
add_test(NAME cet_tests COMMAND test_cet)
