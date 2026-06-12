Output out/09-feeders-throttle.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 28ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=/home/andy-rea/development/loadr.io/target/debug:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# a random feeder + a hard request-rate ceiling (Gatling throttle)"
Enter
Sleep 800ms
Type "cat feeders-throttle.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# 20 VUs, but throttle caps the whole scenario at 30 req/s"
Enter
Sleep 600ms
Type "loadr run feeders-throttle.yaml"
Enter
Sleep 11s
Type "# observed ~30/s — the ceiling held no matter how many VUs pushed"
Enter
Sleep 4s
