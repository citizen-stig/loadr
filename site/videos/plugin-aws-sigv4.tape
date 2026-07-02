# Source: examples/plugins/aws-sigv4.yaml · plugin: aws-sigv4 (service) · backend: none (signs requests)
Output out/plugin-aws-sigv4.mp4
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
Type "# SigV4-sign S3 (and other AWS) requests with a per-request signer hook"
Enter
Sleep 800ms
Type "loadr plugin install aws-sigv4"
Enter
Sleep 3s
Type "cat aws-sigv4.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch each request get an Authorization + X-Amz-Date header stamped on before it leaves"
Enter
Type "loadr run aws-sigv4.yaml"
Enter
Sleep 11s
Type "# every request signed inline — the sigv4_signatures counter climbs 1-per-request, gated by thresholds"
Enter
Sleep 4s
