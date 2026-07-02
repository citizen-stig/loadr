# Source: examples/plugins/css-select.yaml · plugin: css-select (extractor) · backend: httpbin (HTML)
Output out/plugin-css-select.mp4
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
Type "# css-select: a CSS-selector extractor — pull text or an attribute out of rendered HTML into a ${var}"
Enter
Sleep 800ms
Type "loadr plugin install css-select"
Enter
Sleep 3s
Type "cat css-select.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the extractor parse the HTML body, apply the selector, and feed the first match into a later request"
Enter
Type "loadr run css-select.yaml"
Enter
Sleep 11s
Type "# no metric family of its own — the selector's first match flowed into ${var}, and the http_* requests that reused it passed"
Enter
Sleep 4s
