# Advisor resource safety and recovery

Git Slop does not require Ollama or any model runtime for scans, reports,
policies, plans, or provider-free advisor context. Public inference is disabled
while the checked-in advisor gate is `defer`.

## If memory pressure is occurring now

1. Cancel the inference client with Ctrl-C. Closing the client may not stop a
   separately operated provider, so inspect that runtime independently.
2. On macOS, open Activity Monitor's Memory tab or inspect the largest resident
   processes without changing them:

   ```sh
   ps -axo pid,rss,command | sort -nr -k2 | head
   sysctl vm.swapusage
   vm_stat
   ```

3. If Ollama owns the workload, ask Ollama to stop only the exact model you
   recognize:

   ```sh
   ollama ps
   ollama stop <exact-model-name>
   ```

4. If a Homebrew-managed Ollama service is still running and you intentionally
   own that service, stop it:

   ```sh
   brew services stop ollama
   ```

5. Recheck process memory and swap. Do not retry the same workload on a host
   that failed a capacity gate or experienced an out-of-memory termination.

Do not use broad process kills or recursive deletion commands. Confirm the
exact process, model, service, and package before changing them.

## Remove an unwanted local runtime

These are operator actions outside Git Slop. First verify what is installed:

```sh
brew list --versions ollama
ollama list
```

To remove one exact downloaded model, use the runtime's model-aware operation:

```sh
ollama rm <exact-model-name>
```

To uninstall a Homebrew package after its service is stopped:

```sh
brew uninstall ollama
brew cleanup ollama
```

Do not recursively delete `~`, a home directory, or an unresolved model path.
If another application installed the runtime, use that application's supported
uninstaller instead of the Homebrew commands.

## Verify recovery

On macOS, verify that no unexpected runtime process remains and that pressure
has stopped growing:

```sh
pgrep -fl ollama
sysctl vm.swapusage
vm_stat
```

Swap may not immediately shrink after the workload exits; the important first
checks are that the provider process is gone, available memory has recovered,
and swap use is no longer increasing. Restart the computer only if normal
operator controls cannot recover the system.

If you need a machine-readable eligibility check after recovery, use only the
provider-free capacity command:

```sh
cargo xtask advisor-capacity \
  --model openai/gpt-oss-safeguard-20b \
  --model-size-bytes 13793441254 \
  --estimated-peak-memory-bytes 17179869184 \
  --format json
```

It reads host memory and swap state without reading a repository report or
contacting a provider. It reports every blocker rather than hiding later
failures behind the first one; validate the JSON form with `git slop schema
advisor-capacity`. An ineligible result is expected on the recorded 16-GB M2
Air and is a stop signal, not a reason to try the full benchmark.

## Future benchmark requirements

Never rerun the 20B Safeguard matrix on the recorded 16-GB M2 Air. A future run
must use a separately provisioned dedicated host, explicit provider and model
identity, independently reviewed model and peak-memory sizes, and the complete
capacity preflight. More than 256 MiB of swap already in use fails before
provider contact. The benchmark aborts when available memory drops below its
reserve or swap growth crosses its fixed limit; an abort is terminal evidence
for that host, not permission to retry. A
`benchmark_child_output_limit` abort likewise stops the matrix rather than
retaining unbounded child output.
