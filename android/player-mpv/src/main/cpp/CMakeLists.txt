cmake_minimum_required(VERSION 3.22.1)
project(jellypilot_player CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Staged by the `bun run task android mpv` pipeline:
#   <MPV_STAGING>/include/mpv/{client.h,render.h,...}
#   <MPV_STAGING>/include/libavcodec/jni.h (+ other ffmpeg headers)
#   <MPV_STAGING>/lib/libmpv.so, libavcodec.so, ...
set(MPV_STAGING "" CACHE PATH "libmpv staging root (include/, lib/)")
if(NOT MPV_STAGING)
  message(FATAL_ERROR "MPV_STAGING not set; run the android mpv pipeline first")
endif()
if(NOT EXISTS "${MPV_STAGING}/lib/libmpv.so")
  message(FATAL_ERROR "libmpv.so missing at ${MPV_STAGING}/lib; run `bun run task android mpv`")
endif()

add_library(mpv SHARED IMPORTED GLOBAL)
set_target_properties(mpv PROPERTIES
  IMPORTED_LOCATION "${MPV_STAGING}/lib/libmpv.so"
  INTERFACE_INCLUDE_DIRECTORIES "${MPV_STAGING}/include")

add_library(avcodec SHARED IMPORTED GLOBAL)
set_target_properties(avcodec PROPERTIES
  IMPORTED_LOCATION "${MPV_STAGING}/lib/libavcodec.so"
  INTERFACE_INCLUDE_DIRECTORIES "${MPV_STAGING}/include")

add_library(jellypilot_player SHARED jellypilot_player.cpp)

target_include_directories(jellypilot_player PRIVATE "${MPV_STAGING}/include")

find_library(log-lib log)
find_library(android-lib android)

target_link_libraries(jellypilot_player
  PRIVATE mpv avcodec ${log-lib} ${android-lib})

# 16 KB page-size compatibility for the JNI library itself.
target_link_options(jellypilot_player PRIVATE "-Wl,-z,max-page-size=16384")
