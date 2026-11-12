# Fragment config.sh boxa panko (chroot obrazu; składa go shrimp-box).
# panko-cleanup.timer włącza shrimp-box ([image] enable). Bez /etc/panko/panko.conf (ustawienie
# wyłączone) jego usługa nic nie robi.
[ -x /usr/bin/panko ] || die "panko missing (box panko was not installed)"
