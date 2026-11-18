# 构建环境（由 run.sh 和执行者 source）
VK_LLVM="C:/Program Files/Microsoft Visual Studio/18/Community/VC/Tools/Llvm/x64/bin"
export LIBCLANG_PATH="$VK_LLVM"
export CC_wasm32_unknown_unknown="$VK_LLVM/clang.exe"
export AR_wasm32_unknown_unknown="$VK_LLVM/llvm-ar.exe"
export RUSTUP_USE_CURL=1
export RUSTUP_AUTO_INSTALL=0
export CARGO_NET_RETRY=10
export CARGO_HTTP_TIMEOUT=120
