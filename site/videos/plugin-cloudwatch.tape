# Source: examples/plugins/cloudwatch.yaml · plugin: cloudwatch (output) · backend: localstack / mock CloudWatch
Output out/plugin-cloudwatch.mp4
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
Type "# stream live loadr metrics into Amazon CloudWatch via signed PutMetricData"
Enter
Sleep 800ms
Type "# no install step — this output lives in the plan's outputs: section, not a URL scheme"
Enter
Sleep 800ms
Type "cat cloudwatch.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the outputs plugin ship each 1s snapshot to CloudWatch as it runs"
Enter
Sleep 800ms
Type "loadr run cloudwatch.yaml"
Enter
Sleep 11s
Type "# every snapshot second became a PutMetricData batch — cloudwatch_metrics_sent climbed, throttles stayed at 0"
Enter
Sleep 4s
