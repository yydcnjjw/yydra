<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Yydra Agent Skills and Eval contract research

_Research date: 2026-09-01. Status: research input for **Define the Agent Skills and Eval contract**. This note records current primary-source facts and bounded implications; it does not choose Yydra's contract._

## 1. The portable Agent Skills floor is deliberately small

### Verified facts

- The current Agent Skills specification defines a skill as a directory with a required `SKILL.md`; scripts, references, assets, and other files are optional. `SKILL.md` is YAML frontmatter followed by unrestricted Markdown instructions. Only `name` and `description` are required. Optional standard fields are `license`, `compatibility`, `metadata`, and `allowed-tools`. ([Agent Skills specification at the reviewed revision](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx))
- `compatibility` is human-readable text for environment requirements. `metadata` is an arbitrary string-to-string map; the specification's `version: "1.0"` is an example under `metadata`, not a normative skill-versioning scheme. `allowed-tools` is explicitly experimental and its support may vary by implementation. ([Agent Skills specification](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx#L21-L30))
- The specified loading model is progressive disclosure: catalog `name` and `description`, load the complete `SKILL.md` when activated, then load referenced resources only as needed. The reference validator checks format and naming rules; it does not establish behavioral equivalence across agents. ([Progressive disclosure and validation](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx#L156-L189))
- Skill installation paths are outside the format specification. The official client guide recommends scanning `.agents/skills/` as a cross-client convention, while also allowing client-specific paths. It leaves discovery, collision precedence within a scope, activation mechanism, frontmatter delivery, permission handling, and cloud provisioning to the client. It also says supported script languages depend on the implementation. ([Official client implementation guide](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/client-implementation/adding-skills-support.mdx))
- The client guide treats repository skills as potentially untrusted and suggests gating them on project trust. Its lenient-loading suggestions intentionally relax strict validation for interoperability, so strict conformance and broad practical loading are distinct policies. ([Trust and lenient parsing guidance](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/client-implementation/adding-skills-support.mdx#L70-L145))

### Implications that the contract decision must address

- Cross-client portability can be claimed for the standard package shape and content, but not automatically for activation timing, tool names, permission semantics, executable dependencies, or identical agent behavior.
- Any client-only frontmatter or experimental `allowed-tools` use needs to be identified as an extension with explicit behavior when a client ignores it. Neither `compatibility` nor `allowed-tools` is a portable security sandbox.
- The open specification does not decide how a Skill version relates to a Framework API, Capability, template, or workspace version. A version stored in `metadata` is data unless Yydra separately defines its meaning, compatibility rule, and enforcement point.
- Conformance validation and behavioral evaluation answer different questions: `skills-ref validate` can reject a malformed package, while only an agent run can show whether the packaged guidance is discovered, activated, and followed in a particular client environment.

## 2. Coding-agent evals should grade the resulting workspace first

### Verified facts

- SWE-bench evaluates a generated patch with hidden `FAIL_TO_PASS` tests for the requested behavior and `PASS_TO_PASS` regression tests; both sets must pass. This grades repository behavior rather than the persuasiveness of the agent's final message. ([OpenAI's SWE-bench Verified methodology](https://openai.com/index/introducing-swe-bench-verified/))
- PaperBench requires a repository and a `reproduce.sh` entrypoint. After the agent stops, the submission is copied to a fresh environment and executed separately; generated files and `reproduce.log` become grading evidence. Separating execution from the agent run makes hard-coded task-time claims less credible. ([PaperBench, sections 2.1-2.4](https://cdn.openai.com/papers/22265bac-3191-44e5-b057-7aaacd8e90cd/paperbench.pdf))
- Artifact graders are only as valid as their task and harness. OpenAI's 2026 SWE-bench audit reports that tests can reject functionally correct solutions, underspecified tasks can have multiple valid interpretations, and OS or Python-version differences can produce spurious failures. It also reports contamination that makes the public benchmark unsuitable for measuring current frontier coding capability. ([Why SWE-bench Verified no longer measures frontier coding capabilities](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/))
- OpenAI's grader documentation supports exact string checks and Python code graders in addition to model graders. It advises using an LLM judge when code falls short, testing that judge against many candidate answers and trusted ground-truth grades, and checking expert-human evaluations for reward hacking. ([OpenAI grader guidance](https://developers.openai.com/api/docs/guides/graders))
- PaperBench calibrates its automated judge against expert-labeled leaf criteria in JudgeEval. The paper reports that its selected judge reaches macro F1 0.83, remains less accurate than an expert, and is nondeterministic because it makes nondeterministic model calls. ([PaperBench, sections 4.2 and 7](https://cdn.openai.com/papers/22265bac-3191-44e5-b057-7aaacd8e90cd/paperbench.pdf))

### Implications that the contract decision must address

- Mechanically observable acceptance criteria can be graded by a fresh, agent-inaccessible harness over the submitted workspace: compile and test outcomes, migrations, generated-contract drift, architecture/dependency constraints, and required files. Conversation traces remain useful diagnostic evidence but are weaker outcome evidence.
- A model judge is relevant only for residual qualities that cannot be expressed as executable or structural checks. Its rubric, model snapshot, parameters, calibration set, disagreement with humans, and repeated-grade variance remain part of the result; a judge score should not silently override a failed mechanical gate.
- Harness failures need a distinct result from agent failures. The baseline workspace and a known-valid reference change should pass in the same frozen environment, and tests should allow every implementation that satisfies the task rather than encode one hidden patch.
- Because the target task is adding a realistic Product Domain module, the held-out checks need to cover both positive behavior and prohibited boundary violations. A build-only pass cannot establish safe use of Framework APIs, Capabilities, persistence, or generated public contracts.

## 3. One run and one model name are not a repeatability contract

### Verified facts

- PaperBench runs each model three times per paper and reports averages with standard error. It records exact model snapshots and reasoning settings, agent scaffold and orchestrator, available tools, task and execution containers, Ubuntu version, GPU, internet/API access, and runtime limits. ([PaperBench, sections 5.1-5.2 and Appendix F](https://cdn.openai.com/papers/22265bac-3191-44e5-b057-7aaacd8e90cd/paperbench.pdf))
- OpenAI's 2026 SWE-bench audit selected cases that a model failed inconsistently over 64 independent runs. This demonstrates that a single pass/fail can hide substantial run-to-run variation even before task and environment defects are separated. ([SWE-bench audit methodology](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/))
- Current OpenAI model guidance recommends changing one instruction or tool group at a time and rerunning the same evals; it also says prompt/tool configuration can materially change coding-agent eval results. ([OpenAI model guidance](https://developers.openai.com/api/docs/guides/latest-model#prompting-best-practices))

### Implications that the contract decision must address

Each retained run needs enough provenance to reproduce or explain it:

- immutable starting-workspace and task-fixture identity;
- exact Skill bytes/version and activation path, plus Framework/CLI/Capability identities;
- agent/client and scaffold version, model snapshot, reasoning/sampling parameters, system/developer instructions, tool schemas, permission policy, budgets, timeouts, and retry policy;
- container/image, OS/architecture, toolchains, dependency locks, services, network policy, credentials/capability availability, and any honored seeds;
- final patch/tree, generated artifacts, command/test output, structured grader results, agent/tool trace, elapsed time, token/tool usage, and infrastructure errors.

The reported smoke-eval result can therefore distinguish at least: harness invalid, agent failed, agent passed mechanical gates, and qualitative review pending/passed. Repeated independent runs should report the run count and outcome distribution, not only the best run or a single aggregate score.

To attribute a change to packaged Skill guidance, otherwise identical runs can vary one factor at a time (for example, the Skill revision or presence) while retaining the same task, workspace, model, scaffold, tools, and harness. This is an experimental-control implication, not a decision here about the required matrix size or pass threshold.

## Source boundary

The Agent Skills facts above use the official specification repository at commit `69ef37e9424c0a7ea9dd2293b559e43ec8176379` (committed 2026-08-09). OpenAI product and evaluation claims use only official OpenAI documentation, publications, and papers. The sources describe formats and evaluation methods; none establishes Yydra's versioning rule, supported client list, smoke-task contents, repetition count, or release threshold.
