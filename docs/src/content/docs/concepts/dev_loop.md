---
title: Dev Loop
description: Routine developer activities.
---

We use the term "dev loop" for routine activities of a software developer or
other users of our build system.

Speeding up the dev loop vastly improves the whole developer experience. Some direct 
advantages are:

1. *Quick Feedback*: By getting feedback on their changes quickly, developers have less
  context switches resulting in less errors.
2. *Quick Bugfixes*: Bug fixes can get rolled out quickly.

## Inner Dev Loop

The "inner dev loop" is what the developer runs with high-frequency,
often on his own device, during development:

1. Adding/modifying some source code.
2. Potentially getting real time IDE feedback.
3. Building code.
4. Running affected tests.
5. Understanding errors/warnings and act on them.

## Outer Dev Loop

The "outer dev loop" is what code changes need to go through
until being deployed to production:

1. Creating a change/pull request.
2. Getting CI feedback.
3. Maybe deploying the change for manual testing.
4. Getting a review of other developers.
5. Merging the change to the development branch after validating it again, rebased on latest changes.
6. Getting the change through various deployment stages to production.
