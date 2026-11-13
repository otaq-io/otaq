# 055 · Otaq never deletes build files in the user's own project folder

- **Date:** 2026-11-13
- **Status:** Accepted
- **Related:** 054 (ticket OTAQ-5)

## Summary

When a new ticket's build is copied from the user's own project folder (no kept build to start from), the files
decision 054 leaves out are only left out of the ticket's copy. Otaq never deletes or changes anything in the user's
own folder; it keeps growing as the user's build tool makes it.

## Decision

- Copying from the user's folder applies decision 054's rule to the copy: files none of the last 5 finished tickets'
  builds used are not copied. With no finished ticket yet, there is nothing to judge by, and the copy is whole, as
  today.
- The user's folder is read only: no file in it is deleted, moved or touched.

## Why

The ticket first noted "the user's own checkout is cleaned too when it is used as the seed". Asked as an end user, the
user chose "only leave them out of the copy": deleting files in the folder they opened would surprise people, and the
ticket still gets a small copy. This replaces that note.
