# Source: loadr record — a live session becomes an auto-correlated scenario.
Output out/54-record.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 26ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] '"
Enter
Type "rm -f session.yaml; python3 recserver.py >/dev/null 2>&1 &"
Enter
Type "sleep 1 && clear"
Enter
Show
Type "# start the recording proxy; drive your journey through it."
Enter
Sleep 900ms
Type "loadr record -l 127.0.0.1:8899 -o session.yaml >/tmp/rec.log 2>&1 &"
Enter
Type "REC=$! ; sleep 1.5"
Enter
Sleep 1.2s
Type "curl -s -x http://127.0.0.1:8899 -X POST http://127.0.0.1:8140/login -d '{}' ; echo"
Enter
Sleep 1.5s
Type "curl -s -x http://127.0.0.1:8899 -X POST http://127.0.0.1:8140/order -H 'content-type: application/json' -d @order.json ; echo"
Enter
Sleep 1.5s
Type "kill -INT $REC ; sleep 1.2"
Enter
Sleep 1.5s
Type "clear ; echo '# the emitted scenario — the token auto-correlated:'"
Enter
Sleep 700ms
Type "sed -n '/POST .login/,/qty/p' session.yaml"
Enter
Sleep 4.5s
