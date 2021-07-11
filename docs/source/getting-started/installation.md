# Installation

Inko consists of two parts: the virtual machine, and the compiler. The virtual
machine is written in [Rust](https://www.rust-lang.org/), while the compiler is
written in [Ruby](https://www.ruby-lang.org/).

!!! note
    We are working towards a self-hosting compiler. Once complete, Ruby is no
    longer required.

!!! note
    When installing Inko from source (either manually or using ivm), Rust 1.52
    or newer is required.

Inko officially supports Linux, macOS, and Windows. BSDs and other Unix-like
operating systems should also work, but are not officially supported at this
time.

Windows users can build Inko using the Visual Studio build tools, or using a
Unix compatibility layer such as [MSYS2][msys2].

Inko requires a 64-bits platform, 32-bits platforms are not supported. Inko also
requires that you use a CPU with AES-NI support. Pretty much all Intel and AMD
x86-64 CPUs since 2010 have AES-NI support, so this should not be a problem.

## Choosing an installation method

The easiest way to install Inko is to use the Inko version manager (ivm). ivm
works on Linux, macOS, and Windows, and makes it easy to install multiple Inko
versions at once. If you decide to use ivm you should read the [ivm installation
instructions](ivm.md#installing) first.

Some Linux package managers also provide packages for Inko, and macOS users can
also install Inko using [Homebrew][homebrew].

If you want to contribute to Inko itself, you must build from source instead.

## Linux

### Using ivm

!!! tip
    Please read the [ivm guide](ivm.md) before using ivm.

First install the necessary dependencies:

=== "Arch Linux"
    ```bash
    sudo pacman -S rust ruby libffi base-devel
    ```
=== "Ubuntu 20.04"
    ```bash
    sudo apt install rust ruby ruby-json libffi7 libffi-dev build-essential
    ```
=== "Alpine"
    ```bash
    sudo apk add rust ruby ruby-json libffi libffi-dev build-base
    ```

You can then install Inko using `ivm install VERSION`, with "VERSION" being the
version you want to install. For example:

```bash
ivm install 0.8.1
```

### Arch Linux

The [AUR](https://aur.archlinux.org/) provides the package `inko-git`, which
will install the latest Git version of Inko. You can install this using your
favourite AUR helper:

=== "yay"
    ```bash
    yay -S inko-git
    ```
=== "pacaur"
    ```bash
    pacaur -S inko-git
    ```
=== "pikaur"
    ```bash
    pikaur -S inko-git
    ```
=== "makepkg"
    ```bash
    git clone https://aur.archlinux.org/inko-git.git
    cd inko-git
    makepkg -si
    ```

If you are unsure about what AUR helper to use, we recommend using
[yay](https://aur.archlinux.org/packages/yay/).

## macOS

### Using ivm

!!! tip
    Please read the [ivm guide](ivm.md) before using ivm.

First install the necessary dependencies:

```bash
brew install ruby rust libffi
```

You can then install Inko using `ivm install VERSION`, with "VERSION" being the
version you want to install. For example:

```bash
ivm install 0.8.1
```

### Using Homebrew

```bash
brew install inko
```

This will install the latest stable version of Inko available in Homebrew.

!!! attention
    The Homebrew formula is maintained by Homebrew and its contributors. For
    issues specific to the formula (e.g. it doesn't work on a certain version of
    macOS), please report issues in the [homebrew-core issue
    tracker](https://github.com/Homebrew/homebrew-core/issues).

## Windows

For Windows we highly recommend using ivm. We recommend using the Visual Studio
build tools, but using [MSYS2][msys2] will also work.

### Using Visual Studio build tools

When using the Visual Studio build tools, you must enable the Visual C++ build
tools feature in the Visual Studio build tools installer. ivm/Inko will not work
without this feature.

For Rust, you'll need to use the `stable-msvc` toolchain. If you have
[rustup](https://rustup.rs/) installed, you can set this toolchain as the
default as follows:

```bash
rustup default stable-msvc
```

Next, open a x64 native tools command prompt.You can then install Inko versions
by running `ivm install X` with "X" being the version to install. For example:

```bash
ivm install 0.8.1
```

### Using MSYS2

First install the necessary dependencies:

```bash
pacman -S ruby libffi
```

For Rust we recommend installing it outside of MSYS2 using rustup, as the MSYS2
version of Rust is not always up to date. When doing so, make sure the default
toolchain is set to `stable-x86_64-pc-windows-gnu`:

```bash
rustup default stable-x86_64-pc-windows-gnu
```

You can then install Inko versions by running `ivm install X` with "X" being the
version to install. For example:

```bash
ivm install 0.8.1
```

## Docker

If you are using [Docker](https://www.docker.com/) or
[Podman](https://podman.io/), you can use our official Docker images. These
images are published on Docker Hub in the [inkolang/inko
repository](https://hub.docker.com/r/inkolang/inko).

!!! tip
    If you are using Linux we recommend using Podman, as Podman can be set up to
    not require root access.

To install Inko 0.8.1, run the following:

=== "Docker"
    ```bash
    docker pull inkolang/inko:0.8.1
    ```
=== "Podman"
    ```bash
    podman pull inkolang/inko:0.8.1
    ```

You can then run Inko as follows:

=== "Docker"
    ```bash
    docker run inkolang/inko:0.8.1 inko --version
    ```
=== "Podman"
    ```bash
    podman run inkolang/inko:0.8.1 inko --version
    ```

A full list of all available tags [is found
here](https://hub.docker.com/r/inkolang/inko/tags).

## Building from source

We recommend users only build from source if they want to contribute to Inko. If
you just want to use Inko, we recommend using ivm instead.

### Installing dependencies

When building from source, a few additional dependencies are necessary. For
example, source builds will compile libffi from source by default, and this
requires some extra dependencies to be installed. All the necessary dependencies
can be installed as follows:

=== "Arch Linux"
    ```bash
    sudo pacman -S coreutils autoconf automake libtool rust ruby git make base-devel
    ```
=== "Ubuntu"
    ```bash
    sudo apt install coreutils autoconf automake libtool rust ruby ruby-json git make build-essential
    ```
=== "Alpine"
    ```bash
    sudo apk add coreutils autoconf automake libtool rust ruby ruby-json git make build-base
    ```
=== "macOS"
    ```bash
    brew install coreutils autoconf automake libtool rust ruby git make
    ```
=== "MSYS2"
    ```bash
    pacman -S coreutils autoconf automake libtool ruby git make
    ```

You can reduce the number of dependencies by dynamically linking Inko against
[libffi][libffi]. How to do so is covered below, but when this is enabled you
only need the following dependencies:

=== "Arch Linux"
    ```bash
    sudo pacman -S coreutils rust ruby git make base-devel
    ```
=== "Ubuntu"
    ```bash
    sudo apt install coreutils rust ruby ruby-json git make build-essential
    ```
=== "Alpine"
    ```bash
    sudo apk add coreutils rust ruby ruby-json git make build-base
    ```
=== "macOS"
    ```bash
    brew install coreutils rust ruby git make
    ```
=== "MSYS2"
    ```bash
    pacman -S coreutils ruby git make
    ```

!!! important
    Dynamically linking against libffi on MSYS2 is not supported.

### Building

When building from source, you can produce a debug build, or a release build.

Debug builds are slow but compile fast and contain debugging symbols. Release
builds are fast, but take a little longer to compile (typically around one
minute).

The commands and their outputs per build type are as follows:

| Build type | Command                 | Executable location   | Build time (from scratch)
|:-----------|:------------------------|:----------------------|:--------
| debug      | `make vm/debug DEV=1`   | `target/debug/inko`   | <= 30 seconds
| release    | `make vm/release DEV=1` | `target/release/inko` | <= 1 minute

### Packaging

If you want to create an Inko package for your favourite package manager, the
build steps are a little different. First you must decide two things:

1. What will the installation prefix be at runtime? This typically is `/usr`,
   but it may be different on some platforms (e.g. `/usr/local`).
1. What is the temporary directory/chroot/jail to install the files into, if
   any?

Let's assume that our prefix is `/usr`, and the temporary directory to install
files into is `./chroot`. First we'll build all the necessary files by running
the following:

```bash
make build PREFIX=/usr
```

Now we can install Inko:

```bash
make install PREFIX=/usr DESTDIR=./chroot
```

!!! tip
    You must specify `PREFIX` for both `make build` and `make install`. The
    `DESTDIR` variable is only to be used when running `make install`.

### Feature flags

You can customise a source installation by enabling certain features by setting
the `FEATURES` Make variable. This variable is set to a comma-separated string
of features to enable. For example:

```bash
make build FEATURES='foo,bar'
```

This would compile the VM with the `foo` and `bar` features enabled.

The following feature flags are available:

| Feature flag  | Default state | Description
|:--------------|:--------------|:--------------
| libffi-system | Disabled      | Dynamically link against [libffi][libffi], instead of compiling it from source.
| jemalloc      | Disabled      | Use [jemalloc][jemalloc] instead of the system allocator.

[homebrew]: https://brew.sh/
[msys2]: http://www.msys2.org/
[libffi]: https://sourceware.org/libffi/
[jemalloc]: http://jemalloc.net/
