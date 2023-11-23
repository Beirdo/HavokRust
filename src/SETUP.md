Install Rust
---

* sudo apt install curl
* curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Install Docker
---

* sudo apt install docker.io
* sudo usermod -a -G docker {insert username}
* log out, log back in (so the group addition is seen)

Install PIP
---

* sudo apt install python3-pip

Install screen
---

* sudo apt install screen

Create a virtualenv
---

* cd {project dir}
* virtualenv venv   // NOTE:  this is already in .gitignore

Install localstack (for testing)
---

* cd {project dir}
* source venv/bin/activate
* pip3 install localstack

Startup localstack
---

* screen
* cd {project dir}
* source venv/bin/activate
* localstack start  // NOTE: do this in its own window/screen window

Build the MUD
---

* cd {project dir}
* cargo build

Start the MUD in Docker
---

* to be continued

