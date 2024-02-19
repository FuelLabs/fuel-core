##
# This is place inside `/etc/profile.d/99-firehose-fuel.sh`
# on built system an executed to provide message to use when they
# connect on the box.
export PATH=$PATH:/app

cat /etc/motd
