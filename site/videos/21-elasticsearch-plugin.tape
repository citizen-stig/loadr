Output out/21-elasticsearch-plugin.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 30ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# Elasticsearch ships as a plugin — talks the REST API over hyper, no openssl"
Enter
Sleep 800ms
Type "loadr plugin install elasticsearch"
Enter
Sleep 4s
Type "clear"
Enter
Type "# an elasticsearch:// request is the operation — index / bulk / search, pooled per VU"
Enter
Sleep 600ms
Type "loadr run elasticsearch.yaml"
Enter
Sleep 12s
Type "# elasticsearch_reqs, elasticsearch_req_duration and elasticsearch_docs — thresholds pass"
Enter
Sleep 4.5s
