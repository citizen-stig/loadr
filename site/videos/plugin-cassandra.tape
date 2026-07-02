# Source: examples/plugins/cassandra.yaml · plugin: cassandra (protocol) · backend: cassandra / scylla
Output out/plugin-cassandra.mp4
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
Type "# cassandra: a native CQL plugin that load-tests Cassandra / ScyllaDB straight over the wire"
Enter
Sleep 800ms
Type "loadr plugin install cassandra"
Enter
Sleep 3s
Type "cat cassandra.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# a cql:// URL routes to the plugin: steady reads + a fixed write rate, gated on p95 latency"
Enter
Type "loadr run cassandra.yaml"
Enter
Sleep 11s
Type "# cassandra_reqs counted every statement; cassandra_req_duration held p95 under the threshold"
Enter
Sleep 4s
