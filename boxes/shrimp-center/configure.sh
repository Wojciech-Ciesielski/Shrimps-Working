# Fragment config.sh boxa shrimp-center (chroot obrazu; składa go shrimp-box).
[ -x /usr/bin/shrimp-center ] || die "shrimp-center missing (box shrimp-center was not installed)"
[ -x /usr/sbin/efibootmgr ] || [ -x /usr/bin/efibootmgr ] || die "efibootmgr missing (Shrimp Center switches systems with it)"
