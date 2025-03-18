#!/bin/bash
# Define the order of sections
sections_order=("breaking" "added" "changed" "fixed" "removed")

# Process each section in the specified order
for section in "${sections_order[@]}"; do
    dir_path=".changes/${section}"
    if [ -d "$dir_path" ]; then
        # Collect .md files sorted numerically using version sort
        files=()
        while IFS= read -r -d $'\0' file; do
            files+=("$file")
        done < <(find "$dir_path" -maxdepth 1 -type f -name '*.md' -print0 | sort -V -z)
              
        if [ ${#files[@]} -gt 0 ]; then
            # Capitalize the first letter of the section name
            section_name="$(tr '[:lower:]' '[:upper:]' <<< "${section:0:1}")${section:1}"
            VERSION_TEXT+="\n### $section_name\n"
            for file in "${files[@]}"; do
                filename=$(basename "$file" .md)
                content=$(cat "$file")
                VERSION_TEXT+="- [${filename}](https://github.com/FuelLabs/fuel-core/pull/${filename}): ${content}\n"
            done
        fi
    fi
done
{
    echo 'VERSION_TEXT<<EOF'
    echo -e ${VERSION_TEXT}
    echo EOF
} >> debug.txt