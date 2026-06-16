Output out/22-go-plugin.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 30ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export LOADR_PLUGINS_DIR=/tmp/loadr-demo/plugins && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# loadr plugins aren't Rust-only — this protocol plugin is written in Go"
Enter
Sleep 800ms
Type "grep '//export' go-echo/goecho.go"
Enter
Sleep 3s
Type "clear"
Enter
Type "# build it into a C-ABI shared library with one go command"
Enter
Sleep 700ms
Type "make -C go-echo"
Enter
Sleep 6s
Type "clear"
Enter
Type "# install the freshly-built Go plugin, then drive goecho:// with the real binary"
Enter
Sleep 700ms
Type "loadr plugin install go-echo"
Enter
Sleep 2.5s
Type "loadr run go.yaml"
Enter
Sleep 8s
Type "# goecho_reqs, 0% failed — a Go plugin, loaded over the frozen C ABI"
Enter
Sleep 4.5s
