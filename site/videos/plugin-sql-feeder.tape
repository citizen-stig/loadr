# Source: examples/plugins/sql-feeder.yaml · plugin: sql-feeder (service) · backend: postgres
Output out/plugin-sql-feeder.mp4
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
Type "# sql-feeder: a service plugin runs one SELECT at startup and seeds a data feeder for every VU"
Enter
Sleep 800ms
Type "loadr plugin install sql-feeder"
Enter
Sleep 3s
Type "cat sql-feeder.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the plugin fetch rows once, before any VU starts, then feed them into the run"
Enter
Type "loadr run sql-feeder.yaml"
Enter
Sleep 11s
Type "# sql_feeder_rows loaded the SELECT into memory once — every VU parameterised from live DB data, zero per-request queries"
Enter
Sleep 4s
