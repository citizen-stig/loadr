Output out/19-kafka-plugin.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 30ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# Kafka ships as a plugin — a pure-Rust client, no librdkafka"
Enter
Sleep 800ms
Type "loadr plugin install kafka"
Enter
Sleep 4s
Type "clear"
Enter
Type "# a kafka:// request produces or fetches — broker + topic in the URL, pooled per broker"
Enter
Sleep 600ms
Type "loadr run kafka.yaml"
Enter
Sleep 12s
Type "# kafka_reqs, kafka_req_duration and kafka_msgs — thresholds pass"
Enter
Sleep 4.5s
