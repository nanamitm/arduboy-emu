cmake_minimum_required(VERSION 3.21)

project(arduboy-qt-mobile LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

find_package(Qt6 REQUIRED COMPONENTS Quick QuickControls2 Multimedia Network)
qt_policy(SET QTP0001 NEW)
qt_policy(SET QTP0004 NEW)
qt_policy(SET QTP0002 NEW)

get_filename_component(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/.." ABSOLUTE)
set(FFI_INCLUDE_DIR "${REPO_ROOT}/crates/ffi/include")
find_program(CARGO_EXECUTABLE cargo REQUIRED)

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_PROFILE_FLAG "")
    set(CARGO_PROFILE_DIR debug)
else()
    set(CARGO_PROFILE_FLAG "--release")
    set(CARGO_PROFILE_DIR release)
endif()

# Android uses the Rust cdylib shared by the desktop Qt client.  Select the
# matching Rust target from the Qt/CMake ABI, and let androiddeployqt package
# the resulting .so alongside the Qt libraries.
if(ANDROID)
    # Rust invokes the linker directly, so the generic clang.exe selected by
    # CMake is insufficient: it defaults to the Windows linker.  The NDK
    # target wrapper supplies both the Android triple and its API level.
    get_filename_component(NDK_BIN_DIR "${CMAKE_C_COMPILER}" DIRECTORY)
    if(CMAKE_SYSTEM_VERSION MATCHES "^[0-9]+$")
        set(ANDROID_API_LEVEL "${CMAKE_SYSTEM_VERSION}")
    else()
        set(ANDROID_API_LEVEL 21)
    endif()

    if(CMAKE_ANDROID_ARCH_ABI STREQUAL "arm64-v8a")
        set(RUST_TARGET aarch64-linux-android)
        set(RUST_NDK_TRIPLE aarch64-linux-android)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "armeabi-v7a")
        set(RUST_TARGET armv7-linux-androideabi)
        set(RUST_NDK_TRIPLE armv7a-linux-androideabi)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "x86_64")
        set(RUST_TARGET x86_64-linux-android)
        set(RUST_NDK_TRIPLE x86_64-linux-android)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "x86")
        set(RUST_TARGET i686-linux-android)
        set(RUST_NDK_TRIPLE i686-linux-android)
    else()
        message(FATAL_ERROR "Unsupported Android ABI: ${CMAKE_ANDROID_ARCH_ABI}")
    endif()
    string(TOUPPER "${RUST_TARGET}" RUST_TARGET_ENV)
    string(REPLACE "-" "_" RUST_TARGET_ENV "${RUST_TARGET_ENV}")
    set(RUST_ANDROID_LINKER
        "${NDK_BIN_DIR}/${RUST_NDK_TRIPLE}${ANDROID_API_LEVEL}-clang.cmd")
    if(NOT EXISTS "${RUST_ANDROID_LINKER}")
        message(FATAL_ERROR "Android NDK linker not found: ${RUST_ANDROID_LINKER}")
    endif()
    set(FFI_SHARED "${REPO_ROOT}/target/${RUST_TARGET}/${CARGO_PROFILE_DIR}/libarduboy_ffi.so")
    set(FFI_LINK "${FFI_SHARED}")
    set(CARGO_BUILD_COMMAND
        ${CMAKE_COMMAND} -E env
        "CARGO_TARGET_${RUST_TARGET_ENV}_LINKER=${RUST_ANDROID_LINKER}"
        ${CARGO_EXECUTABLE} build --target ${RUST_TARGET} ${CARGO_PROFILE_FLAG} -p arduboy-ffi)
else()
    set(FFI_SHARED "${REPO_ROOT}/target/${CARGO_PROFILE_DIR}/libarduboy_ffi.so")
    if(WIN32)
        set(FFI_SHARED "${REPO_ROOT}/target/${CARGO_PROFILE_DIR}/arduboy_ffi.dll")
        set(FFI_IMPLIB "${REPO_ROOT}/target/${CARGO_PROFILE_DIR}/arduboy_ffi.dll.lib")
    elseif(APPLE)
        set(FFI_IMPLIB "${FFI_SHARED}")
    else()
        set(FFI_IMPLIB "${FFI_SHARED}")
    endif()
    set(FFI_LINK "${FFI_IMPLIB}")
    set(CARGO_BUILD_COMMAND ${CARGO_EXECUTABLE} build ${CARGO_PROFILE_FLAG} -p arduboy-ffi)
endif()

file(GLOB_RECURSE FFI_RUST_SOURCES
    "${REPO_ROOT}/crates/ffi/src/*.rs"
    "${REPO_ROOT}/crates/core/src/*.rs")
add_custom_command(
    OUTPUT "${FFI_SHARED}"
    COMMAND ${CARGO_BUILD_COMMAND}
    WORKING_DIRECTORY "${REPO_ROOT}"
    DEPENDS ${FFI_RUST_SOURCES}
    COMMENT "Building the Rust FFI library for ${CMAKE_SYSTEM_NAME}"
    VERBATIM)
add_custom_target(arduboy_ffi_mobile_build DEPENDS "${FFI_SHARED}")

qt_add_executable(arduboy-qt-mobile
    src/main.cpp
    src/MobileEmulator.cpp)
add_dependencies(arduboy-qt-mobile arduboy_ffi_mobile_build)
target_include_directories(arduboy-qt-mobile PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
    "${REPO_ROOT}/frontend-qt/src"
    "${FFI_INCLUDE_DIR}")
target_sources(arduboy-qt-mobile PRIVATE
    "${REPO_ROOT}/frontend-qt/src/EmulatorCore.cpp"
    "${REPO_ROOT}/frontend-qt/src/AudioOutput.cpp")
target_link_libraries(arduboy-qt-mobile PRIVATE
    Qt6::Quick Qt6::QuickControls2 Qt6::Multimedia Qt6::Network
    "${FFI_LINK}")

qt_add_qml_module(arduboy-qt-mobile
    URI ArduboyMobile
    VERSION 1.0
    QML_FILES qml/Main.qml)

if(ANDROID)
    set_property(TARGET arduboy-qt-mobile PROPERTY QT_ANDROID_PACKAGE_SOURCE_DIR
        "${CMAKE_CURRENT_SOURCE_DIR}/android")
    # Qt's Android OpenSSL plugin is shipped separately from Qt. The Android
    # SDK package installed by Qt Creator provides these ABI-specific libraries.
    get_filename_component(ANDROID_SDK_DIR "${CMAKE_ANDROID_NDK}/../.." ABSOLUTE)
    set(ANDROID_OPENSSL_DIR
        "${ANDROID_SDK_DIR}/android_openssl/ssl_3/${CMAKE_ANDROID_ARCH_ABI}")
    set(ANDROID_OPENSSL_CRYPTO "${ANDROID_OPENSSL_DIR}/libcrypto_3.so")
    set(ANDROID_OPENSSL_SSL "${ANDROID_OPENSSL_DIR}/libssl_3.so")
    if(NOT EXISTS "${ANDROID_OPENSSL_CRYPTO}" OR NOT EXISTS "${ANDROID_OPENSSL_SSL}")
        message(FATAL_ERROR
            "Android OpenSSL 3 libraries are required for HTTPS ROM links. "
            "Expected ${ANDROID_OPENSSL_DIR}")
    endif()
    set_property(TARGET arduboy-qt-mobile APPEND PROPERTY QT_ANDROID_EXTRA_LIBS
        "${FFI_SHARED}" "${ANDROID_OPENSSL_CRYPTO}" "${ANDROID_OPENSSL_SSL}")
elseif(WIN32)
    add_custom_command(TARGET arduboy-qt-mobile POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${FFI_SHARED}" "$<TARGET_FILE_DIR:arduboy-qt-mobile>")
endif()
