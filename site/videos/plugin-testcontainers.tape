# Source: examples/plugins/testcontainers.yaml · plugin: testcontainers (service) · backend: docker (/var/run/docker.sock)
Output out/plugin-testcontainers.mp4
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
Type "# spin an ephemeral service container up for the run, and down after — no compose file"
Enter
Sleep 800ms
Type "loadr plugin install testcontainers"
Enter
Sleep 3s
Type "cat testcontainers.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# on start() it talks the Docker API over /var/run/docker.sock, waits for readiness, and hands the run the mapped host:port"
Enter
Sleep 800ms
Type "loadr run testcontainers.yaml"
Enter
Sleep 12s
Type "# container created, load run finished, container torn down on stop() — a self-contained fixture"
Enter
Sleep 4s
