//! Generates a minimal, real, buildable Android Gradle project tree.
//!
//! This is the exact recipe verified end-to-end on this project's
//! development machine (see ADR 0011): Android Gradle Plugin 8.7.3,
//! Kotlin Gradle Plugin 2.0.21, explicit JVM target 17 (to avoid a
//! Kotlin/Java compiler target mismatch against whatever JDK is actually
//! installed), and a plugin/dependency repository block pointing at
//! Google's and Maven Central's repositories (required for AGP/Kotlin to
//! resolve at all — the Gradle Central Plugin Repository alone does not
//! carry them).
//!
//! The generated app has exactly one `Activity` showing one `TextView`
//! whose text is the component's rendered message — deliberately as
//! small as `web-static`'s own scope (see ADR 0003/0010): this is a real,
//! working Android app, not a UI framework.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct ProjectSpec<'a> {
    pub package: &'a str,
    pub app_label: &'a str,
    pub min_sdk: u32,
    pub target_sdk: u32,
    pub compile_sdk: u32,
    pub sdk_dir: &'a Path,
    /// Already Kotlin-string-escaped (see `kotlin::render`).
    pub escaped_message: &'a str,
}

/// Writes a complete Gradle project into `project_dir` (which must already
/// exist or be creatable). Returns the path to the generated
/// `MainActivity.kt` purely for tests that want to assert on it directly.
pub fn generate(spec: &ProjectSpec, project_dir: &Path) -> io::Result<PathBuf> {
    let app_dir = project_dir.join("app");
    let package_path = spec.package.replace('.', "/");
    let kotlin_dir = app_dir.join("src/main/kotlin").join(&package_path);
    fs::create_dir_all(&kotlin_dir)?;
    fs::create_dir_all(app_dir.join("src/main/res/values"))?;

    fs::write(project_dir.join("settings.gradle.kts"), SETTINGS_GRADLE_KTS)?;
    fs::write(project_dir.join("build.gradle.kts"), ROOT_BUILD_GRADLE_KTS)?;
    fs::write(project_dir.join("gradle.properties"), GRADLE_PROPERTIES)?;
    fs::write(
        project_dir.join("local.properties"),
        format!(
            "sdk.dir={}\n",
            spec.sdk_dir.display().to_string().replace('\\', "/")
        ),
    )?;

    fs::write(app_dir.join("build.gradle.kts"), app_build_gradle_kts(spec))?;
    fs::write(
        app_dir.join("src/main/AndroidManifest.xml"),
        android_manifest_xml(spec.app_label),
    )?;
    let activity_path = kotlin_dir.join("MainActivity.kt");
    fs::write(
        &activity_path,
        main_activity_kt(spec.package, spec.escaped_message),
    )?;

    Ok(activity_path)
}

const SETTINGS_GRADLE_KTS: &str = r#"pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "omniforge-android-app"
include(":app")
"#;

const ROOT_BUILD_GRADLE_KTS: &str = r#"plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
}
"#;

const GRADLE_PROPERTIES: &str = "android.useAndroidX=false\n";

fn app_build_gradle_kts(spec: &ProjectSpec) -> String {
    format!(
        r#"plugins {{
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}}
android {{
    namespace = "{package}"
    compileSdk = {compile_sdk}
    defaultConfig {{
        applicationId = "{package}"
        minSdk = {min_sdk}
        targetSdk = {target_sdk}
        versionCode = 1
        versionName = "1.0"
    }}
    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }}
    kotlinOptions {{
        jvmTarget = "17"
    }}
}}
"#,
        package = spec.package,
        compile_sdk = spec.compile_sdk,
        min_sdk = spec.min_sdk,
        target_sdk = spec.target_sdk,
    )
}

fn android_manifest_xml(app_label: &str) -> String {
    // `app_label` is written into an XML attribute: the same HTML/XML
    // escaping rule applies (only `&`, `<`, `>`, `"`, `'` are special in
    // an XML attribute value). `app_label` comes from the android-app
    // manifest's own schema-validated `app_label` field (a project author's
    // own file, not end-user input), but escaping it costs nothing and
    // keeps this function correct even if that ever changes.
    fn escape_xml_attr(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                c => out.push(c),
            }
        }
        out
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application android:label="{label}">
        <activity android:name=".MainActivity" android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#,
        label = escape_xml_attr(app_label)
    )
}

fn main_activity_kt(package: &str, escaped_message: &str) -> String {
    format!(
        r#"package {package}

import android.app.Activity
import android.os.Bundle
import android.widget.TextView

class MainActivity : Activity() {{
    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
        val view = TextView(this)
        view.text = "{escaped_message}"
        setContentView(view)
    }}
}}
"#
    )
}
