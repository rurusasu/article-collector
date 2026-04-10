# Changelog

## 0.1.0 (2026-04-10)


### Features

* add devcontainer Dockerfile and configuration ([85b8c4f](https://github.com/rurusasu/article-collector/commit/85b8c4fc58fee83f1a00c39818586853e596be1f))
* add fetch-article.sh with public API routing ([9f2fe2d](https://github.com/rurusasu/article-collector/commit/9f2fe2db63c67a2c9a1f3b04e4ae7da99fc3376f))
* add public gitconfig with gh auth credential helper setup ([58598c9](https://github.com/rurusasu/article-collector/commit/58598c9fe87063b0b570125ce97b011480a1e4e3))
* add Rust CI/CD pipelines (test, build, release) ([#3](https://github.com/rurusasu/article-collector/issues/3)) ([5e23936](https://github.com/rurusasu/article-collector/commit/5e239368cb2e5efc57d7aff12e14bcb09f3fd219))
* add save-and-pr.sh with configurable target repo ([578f724](https://github.com/rurusasu/article-collector/commit/578f724bc98f3e08f2f174e86b2fe8bc0b60ef6c))
* add setup.sh for Claude Code on the Web cloud environment ([317ed47](https://github.com/rurusasu/article-collector/commit/317ed47cd8afb65aad84b328c8cceeb41c349599))
* add translate.sh with configurable LLM API ([c6632ba](https://github.com/rurusasu/article-collector/commit/c6632ba5791412d66b00536ef5c1549e257367bb))
* automate version management with release-please and pre-commit hook ([#4](https://github.com/rurusasu/article-collector/issues/4)) ([b450480](https://github.com/rurusasu/article-collector/commit/b450480b20bc0aab49b8dd848a75d2d52c9e5526))
* initial project structure ([fcc2083](https://github.com/rurusasu/article-collector/commit/fcc2083dc89061158b4f2155c73f2220d860ee25))
* rewrite article-collector pipeline in Rust ([db73f8a](https://github.com/rurusasu/article-collector/commit/db73f8a6e4baec58fa2b5658bc2c39071604f846))


### Bug Fixes

* ensure /tmp/collect dir exists in translate.sh ([6eeecfa](https://github.com/rurusasu/article-collector/commit/6eeecfacf06aeec9c80febbc90f3cdaba1dbd310))
* prevent VS Code credential helper from contaminating repo gitconfig ([#2](https://github.com/rurusasu/article-collector/issues/2)) ([1bcc7ed](https://github.com/rurusasu/article-collector/commit/1bcc7ed4ee7de2969c30e8bde70889e6c6a85e88))
* SAVE_PATH_TEMPLATE expansion bug ([9186b8e](https://github.com/rurusasu/article-collector/commit/9186b8ee98b8acf05109dcddc5b3a0bd569ae41d))
* use GitHub releases fallback for go-task install ([8ea4a55](https://github.com/rurusasu/article-collector/commit/8ea4a553982972b0778cdf0e1c330c291bdf6f06))
