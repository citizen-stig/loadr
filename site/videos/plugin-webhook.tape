# Source: examples/plugins/webhook.yaml · plugin: webhook (output) · backend: a mock HTTP endpoint
Output out/plugin-webhook.mp4
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
Type "# stream loadr's live metrics to any HTTP endpoint via the native webhook output plugin"
Enter
Sleep 800ms
Type "# no install step needed — this output is configured inside the plan's outputs: section, not resolved by URL scheme"
Enter
Sleep 800ms
Type "cat webhook.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch outputs: a type: plugin webhook sink POSTs one JSON snapshot per second to the configured url"
Enter
Type "loadr run webhook.yaml"
Enter
Sleep 11s
Type "# webhook_deliveries climbed once per second with webhook_delivery_errors at zero — every snapshot POSTed"
Enter
Sleep 4s
