#!/bin/bash
# Which of the programs of integration tests (tests/*.rs) a part of the CI runs, written as the `--test NAME` arguments of `cargo test`:
#
#     tools/ci/select_integration_tests.sh all|first|second [FOLDER]
#
# `all` is every program but the four that have steps of their own in .github/workflows/ci.yml (the ones that measure real time, and the one about what ffmpeg itself does).
#
# The Windows column of the CI is cut in three jobs that run side by side (the unit tests, and the integration tests in two parts), because the integration tests took 32 of its 48
# minutes (CI run 172; the tests of the picture engine and of the export run 3 to 5 times slower on Windows than on Ubuntu). `first` is the programs listed below and `second` is
# **every other program**, so that a program of tests that is added later is run by the second part until someone moves it, and is never left out. The list was made from the
# times of run 173 on Windows (the 50 programs ran 1503 seconds): media_export 332 s, media_export_tracks 156 s, media_export_effects 135 s and midi_in_the_sound 87 s are 710 of
# them, and the first part also runs the 66 seconds of the programs that measure real time, so that the two jobs end together. (Run 173 had media_in_place in place of
# media_export_effects: 613 seconds against 890, and the jobs took 13.9 and 18.3 minutes.)
#
# tools/ci/test_select_integration_tests.py checks that `first` and `second` are every program between them and no program twice, and that the four programs that are left out
# are the four that ci.yml runs by name.
set -u

which=${1:-}
folder=${2:-tests}

# Run by steps of their own: they measure real time (a shared machine with a few slow processors cannot be asked to pass them), or what ffmpeg itself does.
separate="application_playback application_sound media_ffmpeg_quirks sound_engine_real_time"
# The long ones (see above).
first="media_export media_export_tracks media_export_effects midi_in_the_sound"

contains() {
    case " $2 " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

case "$which" in
    all | first | second) ;;
    *)
        echo "usage: select_integration_tests.sh all|first|second [FOLDER]" >&2
        exit 2
        ;;
esac

# A name that this script lists but that is not a program of tests any more would quietly make a part smaller than it was meant to be.
for name in $first $separate; do
    if [ ! -f "$folder/$name.rs" ]; then
        echo "select_integration_tests.sh: there is no $folder/$name.rs, which this script names" >&2
        exit 1
    fi
done

selection=""
for file in "$folder"/*.rs; do
    [ -e "$file" ] || continue
    # (Not `basename`: a program of its own for each file, which is slow on Windows.)
    name=${file##*/}
    name=${name%.rs}
    if contains "$name" "$separate"; then
        continue
    fi
    case "$which" in
        first) contains "$name" "$first" || continue ;;
        second) ! contains "$name" "$first" || continue ;;
    esac
    selection="$selection --test $name"
done
echo "${selection# }"
