# Source: examples/plugins/json-schema.yaml · plugin: json-schema (assertion) · backend: httpbin
Output out/plugin-json-schema.mp4
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
Type "# json-schema: gate a response on its whole shape — validate every body against a JSON Schema, not one field at a time"
Enter
Sleep 800ms
Type "loadr plugin install json-schema"
Enter
Sleep 3s
Type "cat json-schema.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch each response body get validated against the compiled schema — a shape mismatch fails the request with the offending JSON path"
Enter
Type "loadr run json-schema.yaml"
Enter
Sleep 11s
Type "# every body conformed to the schema — the checks rate held at 100% and http_req_failed stayed 0"
Enter
Sleep 4s
