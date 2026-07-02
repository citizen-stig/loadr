# Source: examples/plugins/openapi-contract.yaml · plugin: openapi-contract (assertion) · backend: httpbin
Output out/plugin-openapi-contract.mp4
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
Type "# openapi-contract: a pure-Rust assertion that gates each response on its OpenAPI 3 operation — status, headers, and body schema"
Enter
Sleep 800ms
Type "loadr plugin install openapi-contract"
Enter
Sleep 3s
Type "cat openapi-contract.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch each response validated against the operation the spec documents — an undocumented status, a missing header, or a body that breaks the schema fails the request"
Enter
Type "loadr run openapi-contract.yaml"
Enter
Sleep 11s
Type "# no metric family of its own — every response matched its contract, so the checks rate stayed at 1.0 and http_req_failed held at 0"
Enter
Sleep 4s
