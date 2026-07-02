# Source: examples/plugins/redis-loader.yaml · plugin: redis-loader (service) · backend: redis
Output out/plugin-redis-loader.mp4
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
Type "# a service plugin: a distributed shared feeder handing every VU its next value from a Redis list"
Enter
Sleep 800ms
Type "loadr plugin install redis-loader"
Enter
Sleep 3s
Type "cat redis-loader.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# the plugin starts once, binds a feeder endpoint, and pops NEXT from the shared Redis key"
Enter
Type "loadr run redis-loader.yaml"
Enter
Sleep 11s
Type "# 25 VUs drew from one Redis-backed queue — redis_loader_rows counted each value handed out exactly once"
Enter
Sleep 4s
