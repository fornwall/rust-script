#!/usr/bin/env python3
# coding: utf-8

# Copyright ⓒ 2016 Daniel Keep.
#
# Licensed under the MIT license (see LICENSE or <http://opensource.org
# /licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
# <http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
# files in the project carrying such notice may not be copied, modified,
# or distributed except according to those terms.

import os.path
import re
import subprocess
import sys
import yaml
from itertools import chain

LOG_DIR = os.path.join('local', 'tests')

TRACE = os.environ.get('TRACE_TEST_MATRIX', '') != ''
USE_ANSI = True if sys.platform != 'win32' else os.environ.get('FORCE_ANSI', '') != '' or os.environ.get('ConEmuANSI', 'OFF') == 'ON'

def main():
    travis = yaml.load(open('.travis.yml'))
    script = translate_script(travis['script'])
    default_rust_vers = travis['rust']
    # env = {e[0].strip(): e[1].strip() for e in (
    #     e.split('=', maxsplit=1) for e in travis['env'])}

    matrix_includes = travis.get('matrix', {}).get('include', [])

    vers = set(default_rust_vers)
    include_vers = []
    exclude_vers = set()

    if not os.path.exists(LOG_DIR):
        os.makedirs(LOG_DIR)

    for arg in sys.argv[1:]:
        if arg in vers and arg not in include_vers:
            include_vers.append(arg)
        elif arg.startswith('-') and arg[1:] in vers:
            exclude_vers.add(arg[1:])
        else:
            msg("Don't know how to deal with argument `%s`." % arg)
            sys.exit(1)

    if include_vers == []:
        include_vers = default_rust_vers[:]

    rust_vers = [v for v in include_vers if v not in exclude_vers]
    msg('Tests will be run for: %s' % ', '.join(rust_vers))

    results = []
    for rust_ver in rust_vers:
        seq_id = 0
        for env_var_str in travis.get('env', [""]):
            env_vars = parse_env_vars(env_var_str)
            for row in chain([{}], matrix_includes):
                if row.get('rust', None) not in (None, rust_ver):
                    continue

                row_env_vars = parse_env_vars(row.get('env', ""))

                cmd_env = {}
                cmd_env.update(env_vars)
                cmd_env.update(row_env_vars)

                success = run_script(script, rust_ver, seq_id, cmd_env)
                results.append((rust_ver, seq_id, success))
                seq_id += 1

    print("")

    msg('Results:')
    for rust_ver, seq_id, success in results:
        msg('%s #%d: %s' % (rust_ver, seq_id, 'OK' if success else 'Failed!'))

def msg(*args):
    if USE_ANSI: sys.stdout.write('\x1b[1;34m')
    sys.stdout.write('> ')
    if USE_ANSI: sys.stdout.write('\x1b[1;32m')
    for arg in args:
        sys.stdout.write(str(arg))
    if USE_ANSI: sys.stdout.write('\x1b[0m')
    sys.stdout.write('\n')
    sys.stdout.flush()

def msg_trace(*args):
    if TRACE:
        if USE_ANSI: sys.stderr.write('\x1b[1;31m')
        sys.stderr.write('$ ')
        if USE_ANSI: sys.stderr.write('\x1b[0m')
        for arg in args:
            sys.stderr.write(str(arg))
        sys.stderr.write('\n')
        sys.stderr.flush()

def parse_env_vars(s):
    env_vars = {}
    for m in re.finditer(r"""([A-Za-z0-9_]+)=(?:"([^"]+)"|(\S*))""", s.strip()):
        k = m.group(1)
        v = m.group(2) or m.group(3)
        env_vars[k] = v
    return env_vars

def run_script(script, rust_ver, seq_id, env):
    target_dir = os.path.join('target', '%s-%d' % (rust_ver, seq_id))
    log_path = os.path.join(LOG_DIR, '%s-%d.log' % (rust_ver, seq_id))
    log_file = open(log_path, 'wt')
    msg('Running tests for %s #%d...' % (rust_ver, seq_id))
    success = True

    def sub_env(m):
        name = m.group(1) or m.group(2)
        return cmd_env[name]

    log_file.write('# %s #%d\n' % (rust_ver, seq_id))
    for k, v in env.items():
        log_file.write('# %s=%r\n' % (k, v))

    cmd_env = os.environ.copy()
    cmd_env['CARGO_TARGET_DIR'] = target_dir
    cmd_env.update(env)

    for cmd in script:
        cmd = re.sub(r"\$(?:([A-Za-z0-9_]+)|{([A-Za-z0-9_]+)})\b", sub_env, cmd)
        cmd_str = '> multirust run %s %s' % (rust_ver, cmd)
        log_file.write(cmd_str)
        log_file.write("\n")
        log_file.flush()
        success = sh(
            'multirust run %s %s' % (rust_ver, cmd),
            checked=False,
            stdout=log_file, stderr=log_file,
            env=cmd_env,
            )
        if not success:
            log_file.write('Command failed.\n')
            log_file.flush()
            break
    msg('... ', 'OK' if success else 'Failed!')
    log_file.close()
    return success

def sh(cmd, env=None, stdout=None, stderr=None, checked=True):
    msg_trace('sh(%r, env=%r)' % (cmd, env))
    try:
        subprocess.check_call(cmd, env=env, stdout=stdout, stderr=stderr, shell=True)
    except:
        msg_trace('FAILED!')
        if checked:
            raise
        else:
            return False
    if not checked:
        return True

def translate_script(script):
    parts = script.split("&&")
    return [p.strip() for p in parts]

if __name__ == '__main__':
    main()
