# GitBound user guide

This guide explains how to use the GitBound desktop application safely. It
is intended for individual developers who use more than one Git or GitHub
identity on the same computer.

GitBound stores identity settings and key **paths**, not passwords, private
key contents, GitHub tokens, or other credentials. Credentials remain managed
by OpenSSH, Git, Git credential helpers, and GitHub CLI.

**A note on two words for one thing.** The application says _identity_; the
configuration file and the command line say _profile_. They mean the same
thing. The word on screen is "identity" because that is what the concept actually
is; the word in `config.toml` and in `gitbound profile add` stayed "profile",
because changing it would break every existing configuration and every
documented command in exchange for a label.

## Before you begin

Install these programs and ensure they are available on `PATH`:

- Git
- OpenSSH (`ssh` and `ssh-keygen`)
- GitHub CLI (`gh`) when GitHub account checks or switching are needed

Open **Diagnostics** in GitBound to confirm that each dependency is
available.

## First-time setup

On first run GitBound offers two ways to create your first identity.

**Import from a repository** is the better one when you already have a
repository configured the way you want. GitBound reads its author, email,
remote owner, hostname, and signing settings, and pre-fills the form with them —
which is more reliable than asking you to retype your own address. Only the
folder you choose is inspected; nothing else on disk is read.

**Start from scratch** opens the same form empty.

Either way you then work through four steps:

1. **Git identity** — the identity's name, and the author name and email that
   will appear on commits.
2. **GitHub account** — the account this identity expects, its host, and
   optionally the repository owners it is allowed to push to. Listing owners is
   what turns a wrong-account push into a failed check rather than a surprise.
3. **SSH key** — optional. Without a key this identity uses HTTPS and a
   credential helper.
4. **Review** — exactly what will be saved. Nothing has been written until you
   confirm here.

Importing from a repository also binds that repository. Starting from scratch
does not touch any repository, because you have not chosen one.

Binding changes repository-local identity settings. It does not implicitly
switch the active GitHub CLI account.

## The Dashboard

The Dashboard answers one question without any clicking: are you about to commit
as the wrong person? It shows the active identity, the current repository and
its status, and recent repositories.

It is read-mostly by design — looking at it changes nothing. The checks that
contact GitHub CLI and SSH, and the continuous-integration panel, run only when
you press their buttons. GitBound never reaches the network on its own.

## Quick Switch

Press `Ctrl`+`K` (`Cmd`+`K` on macOS) from anywhere to open Quick Switch, or use
the trigger in the title bar.

Choose an identity and then choose what to apply it to:

- **Current repository** binds the selected repository to that identity. This
  changes repository-local Git settings only.
- **GitHub CLI account** switches the account `gh` authenticates as. This is
  machine-wide, and it modifies no repository.

Choosing an identity _arms_ the action and shows exactly what will change.
Applying it takes a second, deliberate action — pressing the same identity
again, or pressing Switch. A keyboard shortcut that changed your commit identity
in one keystroke would be the opposite of what this tool is for.

GitBound does not offer to change your global Git identity. A global switch
silently changes the author on every repository on the machine, including ones
you are not looking at; per-repository binding and directory rules cover the
same ground without that risk.

## Settings

Settings holds three things:

- **Appearance.** Follow the system colour scheme, or choose light or dark
  explicitly. A second accent colour is available. Status colours do not change
  with the accent — a passing check stays green whichever accent you pick. This
  preference is stored on your machine and never written to your configuration
  file.
- **Approved folders.** The folders GitBound is allowed to look inside.
  Nothing else on disk is ever scanned. Revoking one asks for confirmation.
- **About.** Version, configuration path, and schema version.

## Configure SSH authentication

SSH authentication uses a key pair:

- The **private key** remains on your computer. A typical filename is
  `id_ed25519`.
- The matching **public key** ends in `.pub`, such as `id_ed25519.pub`. Add
  this public key to the corresponding GitHub account.

Never upload, paste, or share the private key.

### 1. Check for an existing key

On Windows PowerShell:

```powershell
Get-ChildItem $env:USERPROFILE\.ssh
```

On macOS or Linux:

```console
ls -al ~/.ssh
```

Look for a private/public pair such as `id_ed25519` and `id_ed25519.pub`.

### 2. Generate a key when needed

Replace the example email with an email associated with the intended GitHub
account:

```console
ssh-keygen -t ed25519 -C "you@example.com"
```

When multiple GitHub identities share one computer, give each key a distinct
filename, for example `id_ed25519_personal` and `id_ed25519_work`. Use a secure
passphrase unless your environment has a different security requirement.

See GitHub's official guide to
[generating a new SSH key and adding it to the SSH agent](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/generating-a-new-ssh-key-and-adding-it-to-the-ssh-agent).

### 3. Add the public key to GitHub

In GitHub, open **Settings → SSH and GPG keys → New SSH key**, choose
**Authentication Key**, and paste the contents of the `.pub` file. GitHub's
[adding an SSH key](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account)
guide includes both browser and GitHub CLI instructions.

### 4. Select the private key in GitBound

1. Open **Identities**.
2. Find the identity's card and click **Edit**.
3. Go to the **SSH Key** step.
4. Beside **SSH private key**, click **Browse**.
5. Select the private key file — the file without `.pub`.
6. Continue to **Finish** and click **Save changes**.

GitBound records only the selected path. It does not copy or read the key
contents and does not edit `~/.ssh/config`.

### 5. Test authentication

1. Open **SSH Keys**.
2. Select the identity.
3. Click **Test authentication**.
4. Confirm that the reported GitHub account matches the expected account.

The test is manual and makes one SSH connection. GitHub's SSH service normally
reports successful authentication while returning a non-zero shell exit code;
GitBound recognizes GitHub's authenticated account message.

If the test reports a different account, verify that the profile points to the
correct private key and that its public key was added to the intended GitHub
account.

### 6. Optional: use an SSH host alias

Many people who keep several GitHub accounts on one computer give each one a
`Host` alias in `~/.ssh/config`, so that the remote URL selects the key:

```sshconfig
Host github.com-work
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_work
    IdentitiesOnly yes
```

That alias is not a hostname, and the distinction matters. GitBound keeps the
two apart:

| Field              | Holds             | Used for                                                              |
| ------------------ | ----------------- | --------------------------------------------------------------------- |
| **Hostname**       | `github.com`      | The GitHub CLI, HTTPS remote URLs, and the credential helper.         |
| **SSH host alias** | `github.com-work` | Building and matching SSH remote URLs only. Defaults to the hostname. |

Set the alias in **SSH host alias** and leave **Hostname** as the real GitHub
host. Putting an alias in **Hostname** makes GitBound ask the GitHub CLI about
a host it was never authenticated against, and every check then reports the
account as unauthenticated.

GitBound reads aliases; it never writes `~/.ssh/config`. Create the alias
yourself first, then name it in the profile.

Configurations written before schema 3 that stored an alias in `hostname` are
migrated automatically the first time they are read, which repairs profiles
affected by this.

## Configure commit signing

SSH authentication and commit signing are related but separate settings. A key
that authenticates Git operations does not automatically enable signed
commits.

### SSH signing

SSH signing requires Git 2.34 or later.

1. Add the public key to GitHub as a **Signing Key**. GitHub keeps
   authentication keys and signing keys in separate lists, so a key already
   added for authentication is _not_ usable for signing until it is added again
   as a signing key. Adding it the second time reports `Key is already in use`
   only if you pick the wrong type again -- choose **Signing Key** in the
   dropdown. Until this is done, commits are signed locally but GitHub shows
   them as unverified.
2. In GitBound, open **Identities**, find the identity's card, and click **Edit**.
3. Set **Signing format** to **SSH**.
4. Set **Signing key** to the public key path, such as
   `~/.ssh/id_ed25519_work.pub`.
5. Enable **Require signed commits for bound repositories** when every commit
   in those repositories should be signed.
6. Save the profile and bind or rebind the repository.

GitBound applies `user.signingKey`, `gpg.format`, and `commit.gpgSign` to the
bound repository. Unbinding restores the repository-local values that existed
before binding.

See GitHub's guides to
[telling Git about an SSH signing key](https://docs.github.com/en/authentication/managing-commit-signature-verification/telling-git-about-your-signing-key)
and [commit signature verification](https://docs.github.com/en/authentication/managing-commit-signature-verification/about-commit-signature-verification).

### OpenPGP signing

1. Install GPG and create or import a private GPG key.
2. Add the public GPG key to the matching GitHub account.
3. In GitBound, set **Signing format** to **OpenPGP**.
4. Enter the long GPG key ID in **Signing key**.
5. Enable required signing if desired, save the profile, and bind or rebind the
   repository.

GitBound does not store the private GPG key or its passphrase.

## Discover and bind repositories

1. Open **Repositories**.
2. Click **Add path** and select a folder containing repositories. Approved
   folders can be reviewed and revoked later under **Settings**.
3. Click **Scan**. GitBound scans approved folders only, does not follow
   symlinks, and does not run network checks during scanning.
4. Click a repository to open its details page.
5. Choose an identity and click **Apply configuration**. The first click shows
   exactly what will be written; a second confirms it.

The Repositories table itself never writes to a repository. Every change is made
on the details page, behind that confirmation.

**Commit identity** and **Authentication** on that page are read-outs, not
separate settings. They come from the identity you chose, and are edited on the
identity itself. Letting them be set independently would allow committing as one
person while authenticating as another, which is the exact failure GitBound
exists to prevent.

GitHub CLI account switching remains a separate, explicit action. The app shows
the current and target accounts before switching.

**Check CI** on the Repositories page and on the details page asks GitHub CLI
for recent workflow runs. Like every other network call in GitBound, it
happens only when you press the button. A repository with no workflows, or a
machine without GitHub CLI, reports that in place rather than as an error.

## Use the Status page

The **Status** page compares the selected repository with its bound profile.
Local checks cover the Git author, email, remote, owner policy, SSH command,
and signing configuration.

1. Add and scan a repository under **Repositories**.
2. Open the repository's details page, or use the Dashboard's current
   repository card.
3. Click **Run checks**.
4. Review the expected and actual values.

Checks that contact GitHub or test SSH run only when you ask. Until then they
report as unverified rather than as passing.

If no repository is selected, Status explains how to add and scan one rather
than showing an empty page.

## Unbind a repository

Open the repository's details page, choose **Unbind and restore**, review the
confirmation, and confirm the action. GitBound restores the exact
repository-local values captured before the first bind.

## Troubleshooting

### SSH key cannot be saved

- Select the private key file, not the `.pub` file, for **SSH key path**.
- Confirm that the file still exists and is readable by your user account.
- For SSH signing, use the public `.pub` file in **Signing key**.

### SSH authentication is unavailable

- Confirm OpenSSH appears as available under **Diagnostics**.
- Configure **SSH key path** in the selected profile.
- Add the matching public key to the expected GitHub account.
- Check that the profile hostname is correct. A custom SSH host alias belongs in
  **SSH host alias**, not in **Hostname**: the hostname is the real GitHub host
  the GitHub CLI is authenticated against, and putting an alias there makes the
  account look unauthenticated. Aliases must already exist in your SSH
  configuration; GitBound does not create them.

### SSH authenticates as the wrong account

- Confirm the selected private key belongs to the intended GitHub account.
- Check which account contains the matching public key.
- Avoid sharing one authentication key across identities when account
  separation is required.

### Commits are not signed

- Confirm the signing format matches the configured key.
- For SSH signing, use the public key path and Git 2.34 or later.
- Confirm **Require signed commits** is enabled and the repository has been
  bound or rebound after changing the profile.
- Inspect the repository under **Status** for signing drift.

### GitHub does not show “Verified”

- Add the public key to GitHub as a signing key, not only as an authentication
  key.
- Ensure the commit email belongs to and is verified by the expected GitHub
  account.
- Inspect a new commit; changing settings cannot retroactively sign an unsigned
  commit.

### Status is empty

- Install a current build of GitBound.
- Add and scan a repository, then select it before opening Status.
- If inspection fails, read the persistent error message and run Diagnostics.

## The command line

Everything the desktop application does is also a command, and the two share one
engine — so a check that passes in the app passes on the command line and in a
pipeline, for the same reasons.

```console
gitbound profile add work --github-user alice-company --git-name "Alice" --git-email alice@company.example
gitbound bind work
gitbound status
gitbound check
gitbound doctor
gitbound ci status
```

Note the vocabulary: `profile` on the command line is `identity` in the app.

Exit codes are stable and safe to script against: `0` success, `1` an identity
or policy check failed, `2` invalid input or configuration, `3` a missing
dependency. `--json` is available on `status`, `check`, `doctor`,
`profile list`, `profile show`, `directory list`, and `ssh test`.

Shell completions are generated without touching your shell configuration:

```console
gitbound completions bash > gitbound.bash
gitbound completions powershell > _gitbound.ps1
```

## Checking identity in CI

A pipeline can run the same checks before a change merges, and can additionally
audit who actually authored the commits — evidence your own machine does not
have.

```console
gitbound verify --format github
gitbound audit --range origin/main..HEAD
```

A repository can also commit a `.gitbound.toml` declaring which addresses,
hosts, and owners it accepts. GitBound reads that file and never writes it,
and a repository without one behaves exactly as before.

See [CI.md](CI.md) for the GitHub Action, the report formats, and the policy
file format.

## Safety boundaries

GitBound does not:

- store GitHub tokens, passwords, private key contents, or passphrases;
- automatically switch GitHub CLI accounts during ordinary binding;
- edit `~/.ssh/config`;
- scan outside folders explicitly approved by the user;
- follow symlinks while scanning; or
- run network checks without an explicit user action.
