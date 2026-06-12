Output out/08-flow-control.mp4
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
Type "# loops, conditionals and weighted tasks — Gatling + Locust, in YAML"
Enter
Sleep 800ms
Type "sed -n '14,32p' flow-control.yaml"
Enter
Sleep 6s
Type "clear"
Enter
Type "# while-loop browsing + a 70/25/5 weighted behaviour split"
Enter
Sleep 600ms
Type "loadr run flow-control.yaml"
Enter
Sleep 8s
Type "# every virtual user browsed a random number of pages, then branched by weight"
Enter
Sleep 4s
