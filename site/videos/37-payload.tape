# Source: examples/49-payload-complexity.yaml  ·  fixture: goldmark-probe.yaml  ·  target: Gitea markdown API
Output out/37-payload.mp4
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
Type "# real systems fall over on CRAFTED input, not typical input. loadr generates it."
Enter
Sleep 900ms
Type "loadr payload --list | head -20"
Enter
Sleep 3.5s
Type "clear"
Enter
Type "# a payload is parameterised by one magnitude — here, nesting depth:"
Enter
Sleep 700ms
Type "loadr payload nested-markdown-blockquote:12"
Enter
Sleep 2.5s
Type "clear"
Enter
Type "# the plan embeds that payload in the request body, depth from the swept axis."
Enter
Sleep 900ms
Type "# now scale it against a target and fit the complexity exponent:"
Enter
Sleep 1.2s
Type "loadr sweep goldmark-probe.yaml --var depth=4000,8000,16000,32000,64000 --complexity depth --max-exponent 1.2"
Enter
Sleep 24s
Type "# O(n^1.6) — super-linear. exit 99 fails the build. a DoS caught in one command."
Enter
Sleep 3s
