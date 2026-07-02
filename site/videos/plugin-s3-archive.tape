# Source: examples/plugins/s3-archive.yaml · plugin: s3-archive (output) · backend: minio (S3)
Output out/plugin-s3-archive.mp4
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
Type "# an output plugin that parks the finished run report as one gzip'd object in S3"
Enter
Sleep 800ms
Type "# no install step: this output is wired into the plan's outputs: section, not a URL scheme"
Enter
Sleep 800ms
Type "cat s3-archive.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the outputs: block resolve s3-archive by name and upload on finish"
Enter
Sleep 800ms
Type "loadr run s3-archive.yaml"
Enter
Sleep 11s
Type "# one signed PUT at the end: s3_archive_objects=1, the whole report gzip'd to S3"
Enter
Sleep 4s
