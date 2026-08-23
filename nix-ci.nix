# NixCI reads this file straight out of the forge: it must stay a
# single self-contained plain-data file with no imports.
{
  # The checks only target x86_64-linux; do not spend workers on the
  # other flake systems' devshells.
  systems = ["x86_64-linux"];
  # A cold bootstrap of the whole workspace has to fit the first run.
  timeout = 3600;
  # Keep sibling checks' results visible when one fails.
  fail-fast = false;

  test = {
    # Disabled: the hosted worker's sandbox delivers SIGSYS (signal 31)
    # to the executor's nested seccomp actions, which a real kernel
    # does not. Run the suite via `just test` until a worker without a
    # syscall sandbox is available.
    phloem-host-integration = {
      enable = false;
      package = "phloem-host-tests";
      system = "x86_64-linux";
    };
  };
}
