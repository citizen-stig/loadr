# Source: examples/45-sweep.yaml  ·  fixture: sweep.yaml  ·  target: httpbin
Output out/32-sweep.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 28ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# where's the knee? sweep one plan across a VU matrix"
Enter
Sleep 800ms
Type "loadr sweep sweep.yaml --var vus=2,4,8 --duration 8s"
Enter
Sleep 34s
Type "# per-combo exports land in loadr-sweep/ — diff any two with loadr compare"
Enter
Sleep 800ms
Type "ls loadr-sweep/"
Enter
Sleep 3s
