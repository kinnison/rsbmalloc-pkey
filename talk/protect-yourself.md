title: Protect Yourself From Yourself
class: animation-fade
layout: true

.bottom-bar[
{{title}}
]

---

count: false

# Leader slide

- Press 'b' to toggle 'blackout mode'
- Press 'p' to toggle 'presenter mode'
- Press 'c' to create a clone of this window which will keep up
- Press left/right to move around slides
- The next slide is where the presentation begins

???

This is where presenter notes appear

---

class: impact

# {{title}}

## Daniel Silverstone <br /><tt>&lt;dsilvers@digital-scurf.org&gt;</tt>

???

- Welcome everyone
- Explain that this comes from stuff learned at FOSDEM 2024
- I promise this isn't just me telling you off for getting drunk and losing your laptops.

---

title:
class: middle

## How do you protect yourself from yourself?

???

According to someone on Quora...

---

title: How do you protect yourself from yourself?
class: middle

## Identify your flaws. Accept your flaws. Work to improve and control your flaws.

## Identify your insecurities. Accept your insecurities. Work to eradicate and heal your insecurities.

???

In this context, our flaws are an ever increasing set of attack surfaces, and interested parties who want to attack our software.

---

title: How do you protect yourself from yourself?
class: middle

## Don’t deny the parts of yourself you aren’t happy with; instead, recognize them as valid aspects of who you are, and learn from them. Grow from them.

???

The answer is not to stop producing software (it'd be lovely if it were, but sadly we need it).

---

title: How do you protect yourself from yourself?
class: middle

## Move on from what you cannot change, and find help for what you need to change.

???

So let's consider ways to mitigate attacks and look for things to help us to solve our problems.

---

title:
class: impact

# Setting the scene

???

- For the sake of this talk, let's imagine a putative network service
- We are exposed to the wider internet
- We want to consider security in depth
- Let's assume there's some data we want to protect, perhaps some secret material
- How might we do that? (Invite input from the audience)

---

title: Protecting secrets

- Privilege separation

???

- If we can, we could put the secret material into a separate process
- Or we might move all our protocol parsing into that separate process
- Either way, this separate process would be locked down and independent of the
  process which our attacker might be able to reach
- This is often referred to as privilege separation and involves processes, often different usernames, sometimes seccomp etc. It's very powerful

---

title: Protecting secrets

- Privilege separation
- UNIX permissions

???

- It's possible we can simply lock the secrets away in a different user
- This way our service processes cannot access them at all
- Sadly this results in difficulties accessing the secrets if they were needed

---

title: Protecting secrets

- Privilege separation
- UNIX permissions
- Microservices?

???

- We could put the secrets in another service entirely and access that independently
- This might be very convenient
- This is not very efficient (high overhead)

---

title: Protecting secrets

- Privilege separation
- UNIX permissions
- Microservices?

<!-- -->

What if there was a way to hide stuff from myself?

???

- There is another way though
- There is a mechanism in modern x86_64 CPUs (and powerpc ones I think)
- It's a way to allow you to stick your fingers in your ears and say "lalala I can't see this"

---

class: impact

# Intel MPK, PKU, pkeys, oh my!

???

- Called memory protection keys
- or protection keys for userland
- or just pkeys
- These are a use for some spare bits in the page table mappings (pause, find out if anyone doesn't know what a page table is)
- There are 16 keys available, though the kernel basically reserves one for executable pages
- This leaves fifteen available to your process
- In theory powerpc has 32 keys, (31 for you), but I have no way to test this.

---

title: Intel MPK, PKU, pkeys, oh my!

- Each key is a number from 0 to 15
- Used to label memory pages
- Access controlled from userland

???

- Each key effectively can label memory pages
- Without recourse to system calls, processes can change the limits on their access
- You can deny all access, permit read-only access, or permit all access.
- Thus a very low-overhead way to limit access to pages of memory at runtime
- Super-exciting though, these permissions carry through to kernel space too
  so you can't accidentally `write()` or `send()` secret material buffers
- You may find documentation for these in `man 7 pkeys`

---

title: Where can we find stuff?
class: middle

## `https://www.kernel.org/doc/html/v6.7/core-api/protection-keys.html`

## `https://man7.org/linux/man-pages/man7/pkeys.7.html`

???

- There's documentation online, the kernel explains protection keys
- And there're manpages as mentioned

---

class: impact

# Show me the money

???

- Live demo, of C code

---

class: impact

# Eww, C code. Show me the _real_ money

???

- Live demo, of Rust code

---

title: Call to action
class: middle

## What could you protect with these?

???

- While I don't expect any of you to immediately jump at this stuff
- Not least, literally two architectures (x86_64 and powerpc) support it
- What could you potentially solve by using pkeys?
- Mention <https://secure-rewind-and-discard.github.io/>

---

count: false
class: impact

# Any questions?

???

Obvious any-questionsness

---

class: middle
title: Bonus round

## Can we turn those segfaults into panics?

???

Well, _maybe_
