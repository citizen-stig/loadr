# Source: examples/plugins/oauth2-minter.yaml · plugin: oauth2-minter (service) · backend: a mock OAuth2 token endpoint
Output out/plugin-oauth2-minter.mp4
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
Type "# one shared, auto-refreshed OAuth2 bearer token handed to every VU"
Enter
Sleep 800ms
Type "loadr plugin install oauth2-minter"
Enter
Sleep 3s
Type "cat oauth2-minter.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# the service mints one token on start, then re-mints before expiry"
Enter
Type "loadr run oauth2-minter.yaml"
Enter
Sleep 11s
Type "# 50 VUs rode ONE token — oauth2_token_refreshes ticked, not 50 grants"
Enter
Sleep 4s
