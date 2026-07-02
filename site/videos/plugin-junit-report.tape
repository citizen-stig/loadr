# Source: examples/plugins/junit-report.yaml · plugin: junit-report (output) · backend: none (writes a file)
Output out/plugin-junit-report.mp4
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
Type "# turn every check and threshold into a JUnit testcase your CI test panel can read"
Enter
Sleep 800ms
Type "# no install step needed — this output is configured inside the plan's outputs: section, not resolved by URL scheme"
Enter
Sleep 800ms
Type "cat junit-report.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the outputs: plugin entry emit junit.xml as the run finishes"
Enter
Type "loadr run junit-report.yaml"
Enter
Sleep 11s
Type "# junit.xml written — one testcase per check + threshold, junit_failures mirroring the run's pass/fail"
Enter
Sleep 4s
