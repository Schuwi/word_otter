# Description: Extract a word list from a Wiktionary XML dump.
# Usage: python words_from_wiktionary.py <wiktionary-dump.xml> [output-file, default: wortliste.txt]

# JSON output is formatted as follows:
# [
#     {
#         "word": "word",
#         "part_of_speech": "noun",
#         "meanings": [
#             "meaning 1",
#             "meaning 2"
#         ]
#     }
# ]

import sys
import os
import xml.etree.ElementTree as ET
import re

# Set to True to escape non-ASCII characters in JSON output
json_ensure_ascii = False

def clean_markup(text):
    # Remove Wikitext markup

    # Remove templates
    text = re.sub(r"\{\{([^}])+\}\}", r"", text)
    # Remove links
    text = re.sub(r"\[\[([^|]+)\|([^]]+)\]\]", r"\2", text)
    text = re.sub(r"\[\[([^]]+)\]\]", r"\1", text)
    # Remove bold text
    text = re.sub(r"'''([^']+)'''", r"\1", text)
    # Remove italic text
    text = re.sub(r"''([^']+)''", r"\1", text)
    # Reformat lists
    text = re.sub(r"^:\[([\d]+)\]", r"\1:", text)
    # Remove empty lines
    text = re.sub(r"\n\n", r"\n", text)
    # Remove leading and trailing whitespace
    text = text.strip()
    # Remove references
    text = re.sub(r"<ref[^>]*>.*?</ref>", r"", text)
    return text

def extract_words_from_wiktionary(input_file, output_file="wortliste.txt"):
    # Check for existence of input file
    if not os.path.isfile(input_file):
        print(f"Error: {input_file} does not exist.")
        sys.exit(1)

    # Parse the XML file
    print("Parsing XML file...")
    tree = ET.parse(input_file)
    root = tree.getroot()

    # Use the namespace of the root element
    namespace = root.tag.split("}")[0] + "}"

    # Warn if the XML schema is not the tested version
    if namespace != "{http://www.mediawiki.org/xml/export-0.11/}":
        print("Warning: The input file uses a different XML schema than what was tested.")

    # Check if the XML file is a Wiktionary dump
    if root.tag != f"{namespace}mediawiki":
        print("Error: The input file does not seem to be a valid Wiktionary dump.")
        sys.exit(1)

    # Extract dbname (includes language code) from Wiktionary dump
    db_name = root.find(f"{namespace}siteinfo/{namespace}dbname").text
    print(f"Found Wiktionary database: {db_name}")

    # German Wiktionary
    if db_name == "dewiktionary":
        # Words can be assigned to multiple parts of speech
        ARTICLE_REGEX = re.compile(
            r"== ([a-zA-ZäöüßÄÖÜẞ]+) \(\{\{Sprache\|Deutsch\}\}\) ==\n"
            r"=== (\{\{([^}]+)\}\}[, ]{0,2})"
            r"(?:\{\{([^}]+)\}\}[, ]{0,2})?"
            r"(?:\{\{([^}]+)\}\}[, ]{0,2})?"
            r"(?:\{\{([^}]+)\}\}[, ]{0,2})?"
            r"(?:\{\{([^}]+)\}\}[, ]{0,2})?"
            r"(?:\{\{([^}]+)\}\}[, ]{0,2})?"
            r"==="
        )
        # Include word if it is a noun, adjective, verb or adverb
        POS_FILTER = {
            "Substantiv": 'noun',
            "Adjektiv": 'adjective',
            "Verb": 'verb',
            "Adverb": 'adverb',
        }

        MEANING_REGEX = re.compile(r"{{Bedeutungen}}\n(.*?)(?=\n\n|\n{{Sprache|$)")

        words = []

        print("Extracting words from German Wiktionary dump...")
        # Extract words, part of speech and meaning from Wiktionary dump
        for page in root.findall(f".//{namespace}page"):
            ns = page.find(f"{namespace}ns").text
            if ns == "0":
                text = page.find(f".//{namespace}text").text
                if text:
                    match = ARTICLE_REGEX.search(text)
                    if match:
                        # Extract parts of speech
                        pos_candidates = [match.group(i) for i in range(2, 7) if match.group(i)]
                        parts_of_speech = []
                        for pos in pos_candidates:
                            pos_match = re.search(r"Wortart\|([^|]+)\|Deutsch", pos)
                            if pos_match:
                                parts_of_speech.append(pos_match.group(1))

                        # Extract meanings
                        meaning_match = MEANING_REGEX.search(text)
                        if meaning_match:
                            meanings = meaning_match.group(1).split("\n")
                            meanings = [clean_markup(meaning) for meaning in meanings]
                        else:
                            meanings = []

                        # Extract word
                        word = match.group(1)

                        words.append({'word': word, 'parts_of_speech': parts_of_speech, 'meanings': meanings})

        # Filter words by part of speech
        filtered_words = []
        for word_data in words:
            if any(pos in POS_FILTER for pos in word_data['parts_of_speech']):
                filtered_words.append({
                    'word': word_data['word'],
                    'part_of_speech': ([POS_FILTER[pos] for pos in word_data['parts_of_speech'] if pos in POS_FILTER])[0],
                    'meanings': word_data['meanings']
                })

        # Write words to output file
        print(f"Writing {len(filtered_words)} words to {output_file}...")
        if output_file.endswith(".txt"):
            with open(output_file, "w") as f:
                for word_data in filtered_words:
                    f.write(f"{word_data['word']}\n")
        elif output_file.endswith(".json"):
            import json
            with open(output_file, "w", encoding='utf8') as f:
                json.dump(filtered_words, f, indent=4, ensure_ascii=json_ensure_ascii)
        elif output_file.endswith(".json.gz"):
            import gzip
            with gzip.open(output_file, "wt", encoding='utf8') as f:
                json.dump(filtered_words, f, indent=4, ensure_ascii=json_ensure_ascii)
        else:
            print("Error: Invalid output file format. Use .txt, .json or .json.gz.")
            sys.exit(1)
    else:
        print("Error: The script currently only supports the German Wiktionary (dewiktionary).")
        sys.exit(1)
    
    print("Done.")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Extract a word list from a Wiktionary XML dump.")
        print("")

        print("Usage: words_from_wiktionary.py <wiktionary-dump> [output-file, default: wortliste.txt]")
        print("")
        
        print("wiktionary-dump\t\tis a .xml file from any Wiktionary dump that includes article content.")
        print("output-file\t\tcan be a .txt, .json or .json.gz file. JSON files contain part of speech and meanings.")
        print("")
        print("The script extracts words from the article content of the Wiktionary dump.")
        print("It filters words by part of speech (noun, adjective, verb, adverb) and writes them to the output file.")
        print("")
        print("The script may take a long time and a lot of memory because it parses the entire XML file.")
        print("Since the script needs to parse the raw article content, each language Wiktionary must be handled separately.")
        print("The script currently supports the German Wiktionary (dewiktionary).")
        print("")
        print("Examples:")
        print("python words_from_wiktionary.py dewiktionary-20240901-pages-articles.xml")
        print("python words_from_wiktionary.py dewiktionary-20240901-pages-articles.xml wortliste.json")
        
        sys.exit(1)

    input_file = sys.argv[1]
    output_file = sys.argv[2] if len(sys.argv) > 2 else "wortliste.txt"

    if not output_file.endswith(".txt") and not output_file.endswith(".json") and not output_file.endswith(".json.gz"):
        print("Error: Invalid output file format. Use .txt, .json or .json.gz.")
        sys.exit(1)

    extract_words_from_wiktionary(input_file, output_file)