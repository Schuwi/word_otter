## Generating a word list from a Wiktionary dump

This script generates a word list from a Wiktionary dump file. The script reads the dump file and extracts all words from it. It then writes the words to a file with one word per line. The script is written in Python and uses the `xml.etree.ElementTree` module to parse the XML dump file.

Currently the script supports the extraction of words from the German Wiktionary. I am grateful for any contributions to extend the script to support other languages.

### Usage

To generate a word list from a Wiktionary dump, you need to have a Wiktionary dump file in XML format. You can download Wiktionary dumps from the [Wikimedia Downloads](https://dumps.wikimedia.org/) page.

To run the script, use the following command:

```sh
python generate_word_list.py path_to_dump.xml output_file.txt
```

Replace `path_to_dump.xml` with the path to the Wiktionary dump file and `output_file.txt` (or `output_file.json`) with the path to the output file where the word list will be saved.

**Note:** The script may take some time to process, since the full Wiktionary dumps tend to be quite large.

### Example

Here is an example of how to generate a word list from a Wiktionary dump:

```sh
python generate_word_list.py dewiktionary-20240901-pages-articles.xml wortliste.txt
```

This command will read the `dewiktionary-20240901-pages-articles.xml` dump file and generate a word list in the `wortliste.txt` file.

You can also generate a `.json` file by changing the output file extension to `.json`. The JSON file contains additional information about the words, such as their definitions.

### Dependencies

The script requires Python 3 to run. It uses the `xml.etree.ElementTree` module, which is included in the Python standard library.

### Contributing

If you encounter any issues with the script or have suggestions for improvements, feel free to open an issue or submit a pull request on the [GitHub repository](https://github.com/Schuwi/word_otter).