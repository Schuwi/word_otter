import unittest
from words_from_wiktionary import clean_markup

class TestCleanMarkup(unittest.TestCase):

    def test_remove_templates(self):
        self.assertEqual(clean_markup("This is a {{template}}."), "This is a .")

    def test_remove_links(self):
        self.assertEqual(clean_markup("This is a [[link|text]]."), "This is a text.")
        self.assertEqual(clean_markup("This is a [[link]]."), "This is a link.")

    def test_remove_bold_text(self):
        self.assertEqual(clean_markup("This is '''bold''' text."), "This is bold text.")

    def test_remove_italic_text(self):
        self.assertEqual(clean_markup("This is ''italic'' text."), "This is italic text.")

    def test_reformat_lists(self):
        self.assertEqual(clean_markup(":[1] This is a list item."), "1: This is a list item.")

    def test_combined_markup(self):
        self.assertEqual(clean_markup("This is a {{template}} with a [[link|text]] and '''bold''' text."), "This is a  with a text and bold text.")

if __name__ == '__main__':
    unittest.main()