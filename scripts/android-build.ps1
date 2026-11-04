# Builds the Android debug APK on Windows (see CLAUDE.md "Uruchamianie"). Usage: powershell -File scripts/android-build.ps1 [-Target aarch64|x86_64]
param([string]$Target = "aarch64")
$env:JAVA_HOME = "$env:LOCALAPPDATA\Programs\temurin-21\jdk-21.0.13+9"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:NDK_HOME = "$env:ANDROID_HOME\ndk\30.0.16248370"
$env:ANDROID_NDK_ROOT = $env:NDK_HOME
# OpenSSL for Android needs a perl with Unix paths: Git's perl + a few pure-perl modules.
# Relative path (no drive letter): the MSYS shell would turn "/c/..." into "C:/..." and perl
# splits the list on ":". OpenSSL runs perl in target/<triple>/debug/build/openssl-sys-*/out/openssl-build/build/src.
$perlMods = Join-Path $PSScriptRoot "..\src-tauri\target\perl-mods"
New-Item -ItemType Directory -Force $perlMods | Out-Null
Copy-Item "$env:LOCALAPPDATA\Programs\perl-mods\*" $perlMods -Recurse -Force
$env:PERL5LIB = "../../../../../../../../perl-mods"
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\usr\bin;$env:LOCALAPPDATA\Programs\make-shim;$env:JAVA_HOME\bin;$env:Path"
# OpenSSL's makefile runs in a Unix shell - compiler paths with forward slashes only.
$bin = "$($env:NDK_HOME -replace '\\','/')/toolchains/llvm/prebuilt/windows-x86_64/bin"
$triple = @{ aarch64 = "aarch64-linux-android"; x86_64 = "x86_64-linux-android" }[$Target]
Set-Item "env:CC_$triple" "$bin/clang.exe"
Set-Item "env:AR_$triple" "$bin/llvm-ar.exe"
Set-Item "env:RANLIB_$triple" "$bin/llvm-ranlib.exe"
# Plain clang.exe needs the Android API level in the target (24 = Tauri's minimum).
Set-Item "env:CFLAGS_$triple" "--target=${triple}24"
# Gradle's own downloads (wrapper, Maven) trust the Windows certificate store, not only Java's.
$env:JAVA_OPTS = "-Djavax.net.ssl.trustStoreType=Windows-ROOT"
$env:GRADLE_OPTS = $env:JAVA_OPTS
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$log = npx tauri android build --debug --target $Target --apk 2>&1 | ForEach-Object { "$_" }
$log
$code = $LASTEXITCODE
# Without Windows Developer Mode the CLI cannot symlink the .so into jniLibs. The library is
# already built at that point, so copy it and run Gradle directly, skipping the Rust task
# (it would call the CLI again and fail on the same symlink).
if ($code -ne 0 -and ($log -match "symbolic link")) {
    $abi = @{ aarch64 = "arm64-v8a"; x86_64 = "x86_64" }[$Target]
    $flavor = @{ aarch64 = "Arm64"; x86_64 = "X86_64" }[$Target]
    $android = "$root\src-tauri\gen\android"
    New-Item -ItemType Directory -Force "$android\app\src\main\jniLibs\$abi" | Out-Null
    Copy-Item "$root\src-tauri\target\$triple\debug\libtablica_ofert_lib.so" "$android\app\src\main\jniLibs\$abi\" -Force
    Set-Location $android
    "Symlink not allowed - copied the library, running Gradle directly"
    .\gradlew.bat "assemble${flavor}Debug" -x "rustBuild${flavor}Debug" --console=plain 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
}
"exit=$code"
