sudo iptables -t nat -A POSTROUTING -o eno1 -j MASQUERADE
sudo iptables -A FORWARD -o mybridge0 -j ACCEPT
sudo iptables -A FORWARD -i mybridge0 -j ACCEPT
sudo sysctl net.ipv4.ip_forward=1
