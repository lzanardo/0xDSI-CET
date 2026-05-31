cmake_minimum_required(VERSION 3.16)
project(oxdsi_cet C)
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

option(CET_ENABLE_PTHREADS "Enable optional native pthread parallel H-CET runtime" ON)
option(CET_ENABLE_MMAP_ARENA "Enable optional mmap-backed CET runtime arenas" ON)
option(CET_WARNINGS_AS_ERRORS "Treat compiler warnings as errors" OFF)

set(CET_SOURCES
  c_engine/src/dsl.c
  c_engine/src/graph.c
  c_engine/src/algorithms.c
  c_engine/src/optimizer.c
  c_engine/src/sliding.c
  c_engine/src/runtime.c
  c_engine/src/parallel_hcet.c
)

add_library(oxdsi_cet_static STATIC ${CET_SOURCES})
add_library(oxdsi_cet SHARED ${CET_SOURCES})
set_target_properties(oxdsi_cet PROPERTIES OUTPUT_NAME oxdsi_cet)

target_include_directories(oxdsi_cet_static PUBLIC c_engine/include)
target_include_directories(oxdsi_cet PUBLIC c_engine/include)

target_include_directories(oxdsi_cet_static PRIVATE c_engine/src)
target_include_directories(oxdsi_cet PRIVATE c_engine/src)

if(CET_ENABLE_PTHREADS)
  find_package(Threads REQUIRED)
  target_compile_definitions(oxdsi_cet_static PUBLIC CET_ENABLE_PTHREADS=1)
  target_compile_definitions(oxdsi_cet PUBLIC CET_ENABLE_PTHREADS=1)
  target_link_libraries(oxdsi_cet_static PUBLIC Threads::Threads)
  target_link_libraries(oxdsi_cet PUBLIC Threads::Threads)
endif()

if(CET_ENABLE_MMAP_ARENA)
  target_compile_definitions(oxdsi_cet_static PUBLIC CET_ENABLE_MMAP_ARENA=1)
  target_compile_definitions(oxdsi_cet PUBLIC CET_ENABLE_MMAP_ARENA=1)
endif()

if(CET_WARNINGS_AS_ERRORS)
  foreach(tgt oxdsi_cet_static oxdsi_cet)
    target_compile_options(${tgt} PRIVATE -Wall -Wextra -Werror)
  endforeach()
else()
  foreach(tgt oxdsi_cet_static oxdsi_cet)
    target_compile_options(${tgt} PRIVATE -Wall -Wextra)
  endforeach()
endif()

add_executable(test_cet c_engine/tests/test_main.c)
target_link_libraries(test_cet PRIVATE oxdsi_cet_static)

enable_testing()
add_test(NAME cet_tests COMMAND test_cet)
