# Source: examples/plugins/xpath.yaml · plugin: xpath (extractor) · backend: httpbin (XML)
Output out/plugin-xpath.mp4
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
Type "# pull the first XPath 1.0 match out of an XML response body into a var — pure Rust, no libxml2"
Enter
Sleep 800ms
Type "loadr plugin install xpath"
Enter
Sleep 3s
Type "cat xpath.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the xpath extractor evaluate its expression against httpbin's XML and save the match for the next step"
Enter
Type "loadr run xpath.yaml"
Enter
Sleep 11s
Type "# an extractor emits no metric of its own — a clean match feeds ${...} downstream, keeping http_req_failed at 0 and checks at 100%"
Enter
Sleep 4s
