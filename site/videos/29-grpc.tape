# Source: examples/10-grpc.yaml  ·  fixture: grpc.yaml  ·  target: greeter (harness)
Output out/29-grpc.mp4
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
Type "# gRPC straight from a .proto — no generated stubs to build"
Enter
Sleep 800ms
Type "cat grpc.yaml"
Enter
Sleep 6s
Type "clear"
Enter
Type "# unary calls against a real reflection-enabled greeter service"
Enter
Sleep 800ms
Type "loadr run grpc.yaml"
Enter
Sleep 10s
Type "# descriptors compiled at runtime; responses asserted — checks pass"
Enter
Sleep 4s
