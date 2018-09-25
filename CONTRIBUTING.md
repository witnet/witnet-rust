# Contributing

Find an area you can help with and do it. Open source is about collaboration and open participation. Try to make your code look like what already exists and submit a pull request.

The [list of issues](https://github.com/witnet/rust-witnet/issues) is a good place to start, especially the ones tagged as "help wanted" (but don't let that stop you from looking at others). If you're looking for additional ideas, the code includes `TODO` comments for minor to major improvements. Grep is your friend.

Additional tests are rewarded with an immense amount of positive karma.

More documentation or updates/fixes to existing documentation are also very welcome. However, if submitting a PR consisting of documentation changes only, please try to ensure that the change is significantly more substantial than one or two lines. For example, working through an install document and making changes and updates throughout as you find issues is worth a PR. For typos and other small changes, either contact one of the developers, or if you think it's a significant enough error to cause problems for other users, please feel free to open an issue.

Find us at the [Witnet community Gitter chat room](https://gitter.im/witnet/rust-witnet).

This contribution guide is based on [Grin's](https://github.com/mimblewimble/grin/blob/master/CONTRIBUTING.md). We have lots of respect for that project.

## How to write a git-commit message

The goal of these conventions is keeping a healthy commit history in this project. People interested in the project should be able to get a clear picture of the project status right from our commit history.

> Re-establishing the context of a piece of code is wasteful. We can’t avoid it completely, so our efforts should go to reducing it [as much] as possible. Commit messages can do exactly that and as a result, a commit message shows whether a developer is a good collaborator.
>
> -- [Peter Hutterer](http://who-t.blogspot.com/2009/12/on-commit-messages.html)

### Conventions

* Separate subject from body with a blank line
* Limit the subject line to 50 characters
* Capitalize the subject line
* Do not end the subject line with a period
* Use the imperative mood in the subject line
* Wrap the body at 72 characters
* Use the body to explain _what_ and _why_ instead of _how_
* Categorize changes in the body: feature, break(&lt;category&gt;), fix #&lt;issue no&gt;

Example:

``` text
Demonstrate conventions used in commit messages

This commit is to demonstrate the conventions used in this project 
when writing commit messages.

- feature: *Categorized Changes* Lines that begin with a `- <tag>:` can
be used to categorize changes.

- break(P2P): This item is an example of a *scoped* and categorized
change.
  - **Why** the breaking change was necessary.
  - **What** the users should do about it.

- fix #123

You can also put github-flavored markdown code snippets.
```

Our conventions for writing git-commit messages are based on [Chris Beams' post](https://chris.beams.io/posts/git-commit/) and [git-changelog's project](https://github.com/aldrin/git-changelog/blob/master/src/assets/sample-commit.message).
