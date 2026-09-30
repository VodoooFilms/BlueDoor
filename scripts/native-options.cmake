# llama-cpp-sys forwards CMAKE_* environment variables to CMake. Its upstream
# common library enables HTTPS independently of the old LLAMA_CURL option.
# Inference uses local files; model downloads belong to Rust's TLS client.
set(LLAMA_OPENSSL OFF CACHE BOOL "No native HTTP TLS dependency" FORCE)
set(LLAMA_BUILD_BORINGSSL OFF CACHE BOOL "No fetched TLS library" FORCE)
set(LLAMA_BUILD_LIBRESSL OFF CACHE BOOL "No fetched TLS library" FORCE)

# ggml's per-ISA CPU backend matrix (GGML_CPU_ALL_VARIANTS) only has branches
# for Linux, Android and Apple on ARM; the ARM+Windows combination aborts the
# configure step. llama-cpp-sys forces GGML_CPU_ALL_VARIANTS=ON via
# dynamic-backends, which would win over any GGML_* environment variable, so we
# override the cache entry here (loaded before project(): FORCE beats the -D
# passed by the build script). With the matrix off, ggml builds a single
# generic ggml-cpu module, which BLegal Local Demo's runtime already discovers as
# ggml-cpu*.dll beside the executable.
if (WIN32 AND CMAKE_SYSTEM_PROCESSOR MATCHES "ARM64|aarch64|arm64")
    set(GGML_CPU_ALL_VARIANTS OFF CACHE BOOL "ARM64 Windows: single CPU variant only" FORCE)

    # clang-cl defaults to exceptions disabled (cl.exe enables them via /EHsc
    # implicitly). ggml's gguf.cpp uses try/catch, so enable them explicitly.
    # FORCE on the cache makes the flag apply to every CMake scope (ggml and
    # llama subprojects alike), unlike a plain set() which only shadows locally.
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} /EHsc" CACHE STRING "" FORCE)
endif()
