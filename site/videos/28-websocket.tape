# Source: examples/09-websocket.yaml  ·  fixture: websocket.yaml  ·  target: echo (harness)
Output out/28-websocket.mp4
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
Type "# full-duplex load: open a socket, send frames, assert on replies"
Enter
Sleep 800ms
Type "cat websocket.yaml"
Enter
Sleep 6s
Type "clear"
Enter
Type "# a persistent connection per VU against a real echo server"
Enter
Sleep 800ms
Type "loadr run websocket.yaml"
Enter
Sleep 10s
Type "# frames sent and received, latency measured per message — checks pass"
Enter
Sleep 4s
