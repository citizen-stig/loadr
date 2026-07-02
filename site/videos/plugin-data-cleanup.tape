# Source: examples/plugins/data-cleanup.yaml · plugin: data-cleanup (service) · backend: redis + postgres
Output out/plugin-data-cleanup.mp4
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
Type "# a service plugin that deletes the resources a run creates once the run ends"
Enter
Sleep 800ms
Type "loadr plugin install data-cleanup"
Enter
Sleep 3s
Type "cat data-cleanup.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# VUs push each created order id to the service; stop() tears them all down"
Enter
Type "loadr run data-cleanup.yaml"
Enter
Sleep 11s
Type "# cleanup_resources_deleted matched the orders created — cleanup_errors stayed at 0"
Enter
Sleep 4s
