.PHONY: all build check test install install-caps install-systemd uninstall wipe clean bpf

all: build

bpf:
	@echo "Compiling eBPF C programs into target/bpf..."
	mkdir -p target/bpf
	clang -O2 -target bpf -g -c crates/ebpf-programs/cpu_sched.bpf.c -o target/bpf/cpu_sched.bpf.o
	clang -O2 -target bpf -g -c crates/ebpf-programs/mem_pressure.bpf.c -o target/bpf/mem_pressure.bpf.o
	clang -O2 -target bpf -g -c crates/ebpf-programs/net_io.bpf.c -o target/bpf/net_io.bpf.o
	clang -O2 -target bpf -g -c crates/ebpf-programs/proc_lifecycle.bpf.c -o target/bpf/proc_lifecycle.bpf.o
	@echo "eBPF BPF bytecodes compiled successfully in target/bpf/"

build:
	@echo "Building workspace in release mode..."
	cargo build --release

check:
	@echo "Running cargo check..."
	cargo check

test:
	@echo "Running workspace unit and integration tests..."
	cargo test

install: build install-caps install-systemd
	@echo "Full installation complete."

install-caps:
	@echo "Setting CAP_BPF + CAP_PERFMON on installed binary..."
	sudo cp target/release/conductor /usr/local/bin/daemon-choir
	sudo setcap 'cap_bpf,cap_perfmon=ep' /usr/local/bin/daemon-choir

install-systemd:
	@echo "Installing and enabling systemd user service..."
	mkdir -p $(HOME)/.config/systemd/user
	cp systemd/daemon-choir.service $(HOME)/.config/systemd/user/daemon-choir.service
	systemctl --user daemon-reload
	systemctl --user enable daemon-choir.service
	systemctl --user restart daemon-choir.service

uninstall:
	@echo "Stopping and disabling service..."
	systemctl --user stop daemon-choir.service || true
	systemctl --user disable daemon-choir.service || true
	rm -f $(HOME)/.config/systemd/user/daemon-choir.service
	systemctl --user daemon-reload
	@echo "Removing binaries..."
	sudo rm -f /usr/local/bin/daemon-choir
	sudo rm -f /usr/local/bin/daemon-choir-tui
	@echo "Removing configurations..."
	rm -rf $(HOME)/.config/daemon-choir

wipe:
	@echo "Triggering privacy wipe..."
	rm -rf $(HOME)/.local/share/daemon-choir
	@echo "Data successfully wiped."

clean:
	cargo clean
	rm -rf target/bpf
