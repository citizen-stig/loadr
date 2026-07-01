# Source: examples/11-graphql.yaml  ·  fixture: graphql.yaml  ·  target: httpbin
Output out/30-graphql.mp4
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
Type "# GraphQL queries and mutations with variables, over HTTP"
Enter
Sleep 800ms
Type "cat graphql.yaml"
Enter
Sleep 6s
Type "clear"
Enter
Type "# assert on the JSON data — reuses the same HTTP auth and cookies"
Enter
Sleep 800ms
Type "loadr run graphql.yaml"
Enter
Sleep 10s
Type "# query + mutation both green; data asserted with JSONPath — checks pass"
Enter
Sleep 4s
