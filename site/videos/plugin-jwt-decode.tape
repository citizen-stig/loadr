# Source: examples/plugins/jwt-decode.yaml · plugin: jwt-decode (extractor) · backend: httpbin
Output out/plugin-jwt-decode.mp4
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
Type "# pure-Rust JWT extractor: base64url-decodes a token's payload and pulls a named claim for correlation"
Enter
Sleep 800ms
Type "loadr plugin install jwt-decode"
Enter
Sleep 3s
Type "cat jwt-decode.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the login's bearer token get decoded — the sub claim lands in ${user_id} and feeds the next request"
Enter
Type "loadr run jwt-decode.yaml"
Enter
Sleep 11s
Type "# no metric family of its own — the decoded user_id correlated cleanly, so every whoami check passed"
Enter
Sleep 4s
