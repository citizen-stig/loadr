# Source: examples/plugins/db-seeder.yaml · plugin: db-seeder (service) · backend: postgres
Output out/plugin-db-seeder.mp4
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
Type "# a service plugin that seeds the DB before the run and tears it down after — a known state per run"
Enter
Sleep 800ms
Type "loadr plugin install db-seeder"
Enter
Sleep 3s
Type "cat db-seeder.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# start() runs the setup SQL before any VU; stop() runs teardown after the last VU retires"
Enter
Sleep 800ms
Type "loadr run db-seeder.yaml"
Enter
Sleep 11s
Type "# db_seeder_statements counted every setup + teardown statement — fixtures applied before load, DB clean after"
Enter
Sleep 4s
