# arduboy-qt-mobile — Qt Quick Android client

Touch-first Qt Quick/QML frontend for the shared `arduboy_ffi` Rust library.
It intentionally lives beside the Qt Widgets desktop client: emulation, ROM
formats, rendering, and audio stay in the shared core, while the mobile UI uses
responsive QML controls instead of desktop menus and drag-and-drop.

## Current scope

- QML display, 60 fps emulation loop, and `QAudioSink` audio
- Android-compatible local ROM picker (`.hex`, `.arduboy`, `.elf`)
- Touch D-pad and A/B buttons; keyboard support remains useful in desktop/ADB
  testing
- `arduboy://` deep links, including ProjectABE QR codes, download the linked
  HTTPS HEX into the app cache and load it. QR codes contain the URL, not the
  HEX file itself; phone camera apps can hand the link to this app.
- CMake maps the Qt Android ABI to the matching Rust Android target and adds
  `libarduboy_ffi.so` to the Android package

## Android prerequisites

- Qt 6 Android kit with **Quick**, **Quick Controls**, and **Multimedia**
- Android SDK / NDK / JDK configured in Qt Creator
- Qt Creator's Android **OpenSSL 3** SDK package (needed for HTTPS ROM links)
- Rust targets matching the APK ABIs, for example:

  ```bash
  rustup target add aarch64-linux-android
  ```

Configure this directory with the `Qt 6.x.x Android arm64-v8a` kit in Qt
Creator, then build the generated `arduboy-qt-mobile_make_apk` target for a
test APK or `arduboy-qt-mobile_make_aab` for a Play Store bundle.

The first Android build may require setting a Rust linker compatible with the
installed NDK; the CMake target derives it from Qt's Android compiler. The
resulting `libarduboy_ffi.so` is included through `QT_ANDROID_EXTRA_LIBS`.
