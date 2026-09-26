# Contributing

Thanks for looking. Here is what is and isn't open right now.

## Issues and discussion: yes

- Bugs in the docs, the SDK or (once it exists) the program.
- Questions about integrating: how an instruction works, what an event carries, what a client may and may not do.
- Rule questions. If something in `docs/ARCHITECTURE.md` is ambiguous, that is a bug in the doc.
- Feature requests, especially from people building clients. Tell us what you are building and what is missing.

Open a GitHub issue. There are no templates yet; a clear title and enough detail to reproduce or understand is all we need.

## Pull requests to the program: not in v1

The program holds money, and review capacity is the constraint. Until the first audit is done and published, `programs/mybarpool/` does not accept external pull requests. Open an issue instead; if a change is needed we will make it, credit you, and add a test for it.

## Pull requests elsewhere: small ones, yes

Typos, broken links, doc clarifications, and SDK fixes with a test are welcome as pull requests. Keep them focused. A PR that touches the program will be closed with a pointer to this file.

## Security

Anything that could move, freeze or misdirect funds, or influence a draw or a settlement, goes through [SECURITY.md](SECURITY.md), not an issue.

## Style

- Plain language. The doc is written for a bar owner who wants to know where the money goes and an integrator who wants to know what the program enforces. Both should be able to read it.
- The unit is a **box**, never a square, in every user-facing string and every identifier.
- Boxes are labelled 1–25 anywhere a person sees them.
- Numbers are exact. If you change a fee example, recompute everything that depends on it.

## Licence

By contributing you agree that your contribution is licensed under Apache-2.0 like the rest of the repository.
