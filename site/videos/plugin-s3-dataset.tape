# Source: examples/plugins/s3-dataset.yaml · plugin: s3-dataset (service) · backend: minio (S3)
Output out/plugin-s3-dataset.mp4
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
Type "# drive a run from a CSV/JSONL dataset held in an S3 bucket — no AWS SDK, fetched once at startup"
Enter
Sleep 800ms
Type "loadr plugin install s3-dataset"
Enter
Sleep 3s
Type "cat s3-dataset.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# a service plugin fills the users feeder: it signs a SigV4 GET, parses the object into rows"
Enter
Sleep 800ms
Type "loadr run s3-dataset.yaml"
Enter
Sleep 11s
Type "# s3_dataset_rows loaded from the bucket once — VUs then replayed ${data.users.*} straight from memory"
Enter
Sleep 4s
