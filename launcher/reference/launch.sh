#!/bin/bash

script_dir="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bg_array() {
    local arr=("$@")
    local index=0

    for item in "${arr[@]}"; do
        echo "$index: $item"
        ((index++))
    done
}

divider() {
    echo "------------------"
}

skip_line() {
    echo ""
}

menu() {
    local options=("$@")
    MENU_SELECTION=''
    local option_index=0
    local selected_option_index=-1

    skip_line

    while [ $selected_option_index -lt 0 ] || [ $selected_option_index -ge ${#options[@]} ]; do
        for option in "${options[@]}"; do
            echo "$option_index: $option"
            ((option_index++))
        done

        skip_line
        read -p "Select an option: " selected_option_index

        if [ $selected_option_index -lt 0 ] || [ $selected_option_index -ge ${#options[@]} ]; then
            echo "Invalid selection: $selected_option_index. Please select a number between 0 and $(( ${#options[@]} - 1 ))"
        fi
    done

    MENU_SELECTION="${options[$selected_option_index]}"

    skip_line
}

reset_scores() {
    local scores_url="https://raw.githubusercontent.com/dungeons-of-moria/umoria/master/data/scores.dat"
    local scores_files=("$script_dir/data/scores.dat" "$script_dir/scores.dat")

    for scores_file in ${scores_files[@]}; do
        if [ -f "$scores_file" ]; then
            rm -f "$scores_file"
        fi

        if curl -s -o "$scores_file" "$scores_url"; then
            echo "Successfully updated $(basename "$scores_file")"
        else
            echo "Error: Failed to download $scores_url"
            exit 1
        fi
    done
}

reset_save_file() {
    local save_file_name="game.sav"
    rm -f "$script_dir/$save_file_name"

    if [ $? -eq 0 ]; then
        echo "Successfully removed $save_file_name"
    else
        echo "Error: Failed to remove $save_file_name"
    fi
}

reset() {
    menu "Yes" "No"
    if [ "$MENU_SELECTION" = "Yes" ]; then
        reset_save_file
        reset_scores
    else
        echo "Reset cancelled."
    fi

    skip_line
    splash
}

archive() {
    local save_file_name="game.sav"
    local archive_name="$(date +%Y.%m.%d_%H.%M.%S)"
    local archive_dir="$script_dir/_archive"
    local save_file="$script_dir/$save_file_name"

    if [ ! -f "$save_file" ]; then
        return
    fi

    # Create archive directory if it doesn't exist
    if [ ! -d "$archive_dir" ]; then
        mkdir "$archive_dir"
    fi

    local archive_path="$archive_dir/$archive_name"

    # Copy save file to archive
    cp "$save_file" "$archive_path"
}

launch() {
    cd "$script_dir" || exit 1
    ./umoria

    archive
}

unmark_last_loaded() {
    local last_marked_archive_path="$1"
    local unmarked_archive_path="$(echo "$archive_dir/$last_marked_archive" | sed 's/_last_loaded$//')"

    mv "$last_marked_archive_path" "$unmarked_archive_path"
}

mark_last_loaded() {
    local archive_dir="$script_dir/_archive"
    local archive_to_mark="$1"
    local last_marked_archives=($(ls -1 "$archive_dir" | grep -E '_last_loaded$'))

    # Unmark all last loaded archives, if any
    if [ ${#last_marked_archives[@]} -gt 0 ]; then
        for last_marked_archive in "${last_marked_archives[@]}"; do
            unmark_last_loaded "$archive_dir/$last_marked_archive"
        done
    fi

    # Mark selected archive as last loaded
    mv "$archive_dir/$archive_to_mark" "$archive_dir/${archive_to_mark}_last_loaded"

    echo "${archive_to_mark}_last_loaded"
}

load() {
    local save_file="$script_dir/game.sav"
    local archive_dir="$script_dir/_archive"
    local archive_files=($(ls -1 -t "$archive_dir"))
    local archive_count=${#archive_files[@]}

    if [[ ! -d "$archive_dir" || $archive_count -eq 0 ]]; then
        echo "No saved games found. Please archive a save first."
        return
    fi

    divider
    skip_line
    menu "${archive_files[@]}"

    local selected_archive="$MENU_SELECTION"

    selected_archive="$(mark_last_loaded "$selected_archive")"

    rm -f "$save_file"

    cp "$archive_dir/$selected_archive" "$save_file"

    launch
}

splash() {
    echo "Welcome to Umoria!"
    divider

    menu "Resume" "Load" "Reset" "Exit"

    case "$MENU_SELECTION" in
        "Resume")
            launch
            ;;
        "Load")
            load
            ;;
        "Reset")
            reset
            ;;
        "Exit")
            echo "Goodbye."
            exit
            ;;
        *)
            echo "Invalid selection: $MENU_SELECTION."
            splash
    esac
}

splash "$@"
