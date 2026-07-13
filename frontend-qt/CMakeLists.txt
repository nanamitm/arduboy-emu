cmake_minimum_required(VERSION 3.21)

project(arduboy-qt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# Qt6: Widgets for the UI, Multimedia for QAudioSink.
find_package(Qt6 REQUIRED COMPONENTS Widgets Multimedia)

# ── Rust FFI library (arduboy_ffi) ───────────────────────────────────────────
# Built out-of-tree by cargo. We drive it from CMake so a single build produces
# both the Rust cdylib and the Qt executable, then copy the DLL beside the exe.
get_filename_component(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/.." ABSOLUTE)
set(FFI_INCLUDE_DIR "${REPO_ROOT}/crates/ffi/include")
set(CARGO_TARGET_DIR "${REPO_ROOT}/target/release")

# Debug vs release cargo profile follows CMAKE_BUILD_TYPE.
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_PROFILE_FLAG "")
    set(CARGO_TARGET_DIR "${REPO_ROOT}/target/debug")
else()
    set(CARGO_PROFILE_FLAG "--release")
endif()

if(WIN32)
    set(FFI_IMPLIB "${CARGO_TARGET_DIR}/arduboy_ffi.dll.lib")
    set(FFI_SHARED "${CARGO_TARGET_DIR}/arduboy_ffi.dll")
elseif(APPLE)
    set(FFI_IMPLIB "${CARGO_TARGET_DIR}/libarduboy_ffi.dylib")
    set(FFI_SHARED "${FFI_IMPLIB}")
else()
    set(FFI_IMPLIB "${CARGO_TARGET_DIR}/libarduboy_ffi.so")
    set(FFI_SHARED "${FFI_IMPLIB}")
endif()

find_program(CARGO_EXECUTABLE cargo REQUIRED)

# Rebuild the Rust lib whenever its sources change.
file(GLOB_RECURSE FFI_RUST_SOURCES
    "${REPO_ROOT}/crates/ffi/src/*.rs"
    "${REPO_ROOT}/crates/core/src/*.rs")

add_custom_command(
    OUTPUT "${FFI_IMPLIB}"
    COMMAND "${CARGO_EXECUTABLE}" build ${CARGO_PROFILE_FLAG} -p arduboy-ffi
    WORKING_DIRECTORY "${REPO_ROOT}"
    DEPENDS ${FFI_RUST_SOURCES}
    COMMENT "Building Rust FFI library (arduboy_ffi)"
    VERBATIM)

add_custom_target(arduboy_ffi_build DEPENDS "${FFI_IMPLIB}")

# ── Qt executable ────────────────────────────────────────────────────────────
add_executable(arduboy-qt WIN32
    src/main.cpp
    src/MainWindow.cpp
    src/EmulatorCore.cpp
    src/DisplayWidget.cpp
    src/AudioOutput.cpp)

add_dependencies(arduboy-qt arduboy_ffi_build)

target_include_directories(arduboy-qt PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
    "${FFI_INCLUDE_DIR}")

target_link_libraries(arduboy-qt PRIVATE
    Qt6::Widgets
    Qt6::Multimedia
    "${FFI_IMPLIB}")

# On Windows the cdylib needs its runtime DLLs (ws2_32, userenv, etc. from the
# Rust std) — those are already satisfied by the import lib's dependencies.
if(WIN32)
    # Copy the Rust DLL next to the executable so it loads at runtime.
    add_custom_command(TARGET arduboy-qt POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${FFI_SHARED}" "$<TARGET_FILE_DIR:arduboy-qt>"
        COMMENT "Copying arduboy_ffi.dll next to arduboy-qt.exe"
        VERBATIM)
endif()
