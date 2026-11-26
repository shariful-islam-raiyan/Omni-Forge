# hello-android

A minimal, real Android app built by the `android-gradle` adapter —
proof that `omni build android` actually invokes a real Gradle, produces
a real signed debug APK, and that the APK genuinely installs the
project's declared message. See
[ADR 0011](../../docs/decisions/0011-android-gradle-adapter.md) for the
full design and exactly what is and isn't covered.

## What's real here

- `lib/components/ui.greeting/` is a plain `ui` component — a
  `component.yaml` declaring one input (`message`) and a tiny
  `message.kt.tmpl` template, the Android equivalent of `web-static`'s
  `.html.tmpl` (see
  [docs/decisions/0003-template-only-rendering.md](../../docs/decisions/0003-template-only-rendering.md)):
  fixed `{{field}}` substitution only, HTML-escaping's direct analogue for
  a Kotlin string literal instead (`crates/omni-android-adapter/src/kotlin.rs`).
- `android/app.yaml` is the target's entry file (schema
  `omniforge.dev/android-app/v1`): package name, app label, and the root
  component + its inputs.
- `omni build android` generates a complete, minimal Gradle project
  (one `Activity`, one `TextView`) into `.omniforge/android-build/android/`,
  genuinely invokes a real, locally-discovered Gradle (`omni doctor` shows
  exactly which SDK/JDK/Gradle it found), and copies the resulting real
  `app-debug.apk` into `dist/android/` — verified as a real zip archive
  containing `AndroidManifest.xml` before being reported as a success.

## Building it

```sh
cd examples/hello-android
omni doctor               # confirms the Android SDK/JDK/Gradle this will use
omni check                # fast, offline validation (no Gradle invoked)
omni build android        # genuinely invokes Gradle; produces dist/android/app-debug.apk
```

Requires a real Android SDK (with at least one platform + build-tools
installed), a JDK, and a Gradle distribution discoverable on this machine
— see [ADR 0011](../../docs/decisions/0011-android-gradle-adapter.md)'s
"Toolchain discovery" section for exactly how each is found, and what to
set (`ANDROID_HOME`, `JAVA_HOME`, `GRADLE_HOME`) if `omni doctor` reports
one missing. The first build on a machine that has never resolved the
Android Gradle Plugin or Kotlin Gradle Plugin before will be slower
(Gradle downloads them from Google's/Maven Central's repositories);
repeated builds reuse Gradle's own incremental build state.

Inspect the real artifact:

```sh
unzip -l dist/android/app-debug.apk    # a real zip: AndroidManifest.xml, classes.dex, ...
cat dist/android/build-manifest.json
```

## What this does and doesn't prove

- **Does**: a real `.kt` source file is generated from a component
  template and compiled, by a real Gradle + Android Gradle Plugin + Kotlin
  Gradle Plugin, into a real, valid, debug-signed APK — checked by
  re-opening it as a zip archive and confirming `AndroidManifest.xml` and
  `classes.dex` are genuinely present (not merely checking the file
  exists).
- **Doesn't**: install or run the app on an emulator/device (no device/
  emulator lifecycle management exists in this adapter yet), produce a
  release-signed build or an `.aab`, or support anything beyond one
  `Activity` with one `TextView`. See ADR 0011's Consequences section for
  the complete list of what's intentionally not here yet.
