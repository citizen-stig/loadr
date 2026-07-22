# Source: loadr gen — an API contract becomes a runnable, fuzzed scenario.
Output out/51-gen.mp4
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
Type "# you have an OpenAPI spec but no traffic yet. loadr gen writes the test for you."
Enter
Sleep 1s
Type "loadr gen openapi petstore.yaml -o plan.yaml"
Enter
Sleep 2.5s
Type "head -22 plan.yaml"
Enter
Sleep 4s
Type "clear"
Enter
Type "# --fuzz adds spec-invalid + adversarial variants, gated on 'never a 5xx':"
Enter
Sleep 1s
Type "loadr gen openapi petstore.yaml --fuzz | grep -A2 'fuzz:' | head -9"
Enter
Sleep 4s
Type "# a contract in, a load-and-robustness test out."
Enter
Sleep 2s
