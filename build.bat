@echo off
setlocal enabledelayedexpansion
chcp 65001 >nul
REM ============================================================
REM AI 求职助手 - 一键构建（产出单一 exe）
REM
REM 流程：
REM   1. 构建前端（Vite -> web/dist）
REM   2. PyInstaller 打包 Python 后端 -> backend/dist/aja-backend.exe
REM   3. 把后端复制到 backend-exe/（Rust 用 include_bytes! 嵌进主 exe）
REM   4. 生成图标
REM   5. cargo tauri build -> 单一 exe + 安装包
REM
REM 最终产物：
REM   src-tauri\target\release\ai-job-assistant.exe        <- 单文件，双击即用
REM   src-tauri\target\release\bundle\nsis\*.exe           <- 安装包
REM ============================================================

set ROOT=%~dp0
cd /d "%ROOT%"

echo.
echo ============================================================
echo  [1/5] 构建前端
echo ============================================================
cd /d "%ROOT%web"
if not exist node_modules (
    echo 安装前端依赖...
    call npm install
    if errorlevel 1 goto :fail
)
call npm run build
if errorlevel 1 goto :fail

echo.
echo ============================================================
echo  [2/5] 打包 Python 后端
echo ============================================================
cd /d "%ROOT%backend"
python -m pip install --quiet -r requirements.txt
if errorlevel 1 goto :fail
python -m pip install --quiet pyinstaller
if errorlevel 1 goto :fail
python -m PyInstaller aja-backend.spec --noconfirm --clean
if errorlevel 1 goto :fail

echo.
echo ============================================================
echo  [3/5] 准备内嵌后端
echo ============================================================
if not exist "%ROOT%backend-exe" mkdir "%ROOT%backend-exe"
copy /Y "%ROOT%backend\dist\aja-backend.exe" "%ROOT%backend-exe\aja-backend.exe" >nul
if errorlevel 1 goto :fail
echo 后端已就位: backend-exe\aja-backend.exe

echo.
echo ============================================================
echo  [4/5] 生成图标
echo ============================================================
cd /d "%ROOT%"
python tools\make_icons.py
if errorlevel 1 goto :fail

echo.
echo ============================================================
echo  [5/5] 构建 Tauri 主程序（含内嵌后端）
echo ============================================================
cd /d "%ROOT%src-tauri"
set PATH=%PATH%;%USERPROFILE%\.cargo\bin
cargo tauri build
if errorlevel 1 (
    echo cargo tauri build 失败，尝试 npx 方式...
    cd /d "%ROOT%web"
    call npx --yes @tauri-apps/cli@^2 build
    if errorlevel 1 goto :fail
)

echo.
echo ============================================================
echo  构建完成
echo ============================================================
echo  单文件程序: %ROOT%src-tauri\target\release\ai-job-assistant.exe
echo  安装包    : %ROOT%src-tauri\target\release\bundle\nsis\
echo ============================================================
goto :eof

:fail
echo.
echo [错误] 构建失败，请检查上方输出。
exit /b 1
