import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;

public class SubtitlesSearch {
	public static void main(String[] args) throws Exception {
		listFile(new File(SubtitlesFormat.OUTPUT_ROOT + SubtitlesFormat.INPUT_ROOT), "");
	}

	private static void search(File file, String tab) throws Exception {
		try (InputStream inputStream = new FileInputStream(file.getAbsolutePath()); InputStreamReader inputStreamReader = new InputStreamReader(inputStream); BufferedReader reader = new BufferedReader(inputStreamReader)) {
			String line = null;
			List<String> lines = new ArrayList<String>();
			while ((line = reader.readLine()) != null) {
				lines.add(line);
			}
			for (int index = 0; index < lines.size(); index++) {
				line = lines.get(index);
				if (filter(line, index == 0 ? "" : lines.get(index - 1))) {
					System.err.println(file.getName() + tab + line);
				}
				String[] array = line.split(SubtitlesFormat.SEPARATOR_REGEX);
				if (line.startsWith("Dialogue") && array.length != 2 && !array[0].equals("")) {
					if (!line.contains("(") && !line.contains("¡¶") && !line.contains("[")) {
						// TODO
						System.out.println(file.getName() + tab + line);
					}
				}
			}
		}
	}

	private static boolean filter(String line, String lastLine) {
		for (String white : new String[] { "[Script Info]", "[V4+ Styles]", "[Events]", "'Cause", "'Scuse", "'Til" }) {
			if (line.toLowerCase().contains(white.toLowerCase())) {
				return false;
			}
		}
		for (String language : new String[] { "Óï]", "Öä]" }) {
			if (line.contains(language)) {
				return false;
			}
		}
		for (String black : new String[] { "Ü³", "Á¨", "÷á", " ­", "\n", ",0000,", ",\"", "eah\".", "fnCronos Pro Subhead" }) {
			if (line.contains(black) && !line.contains(",,\"")) {
				return true;
			}
		}
		for (String symbol : new String[] { ".,", ",.", "¡¾", "--", "£®", "#", "[", "]", "?'", "!'", "  ", "--", "¡­¡­", "~", ".!", ".?", "!.", "?.", "? )", "! )", ". )", ". ?", ". !", ",,{", ",,'", "?'", "? '", "!'", "! '" }) {
			if (line.contains(symbol)) {
				return true;
			}
		}
		if (line.contains(",,-") && line.split("- ").length > 4 && line.split("- ").length % 2 != 1) {
			return true;
		}
		if (!line.contains(",,-") & line.contains("- ")) {
			return true;
		}
		if (line.contains(".\"") && !line.contains("...\"")) {
			return true;
		}
		if (line.contains(",Note,") && line.contains("\\N")) {
			return true;
		}
		if (line.contains("¡Ó") && line.split("¡Ó").length != 2 && line.split("¡Ó").length != 4) {
			return true;
		}
		if (line.contains("¡Ó") && !line.endsWith("¡Ó")) {
			return true;
		}
		if (line.contains("£¬{") || line.endsWith("£¬")) {
			return true;
		}
		if (Pattern.compile("[0-9]+( )+[0-9:]+").matcher(line).find()) {
			return true;
		}
		if (line.contains("}")) {
			String english = line.substring(line.indexOf("}") + 1);
			if (Character.isLowerCase(english.charAt(0)) && !lastLine.endsWith(",") && !lastLine.endsWith("...")) {
				System.out.println(lastLine.substring(12).replace("Default,,0,0,0,,", "") + "-----" + line.substring(line.indexOf("}") + 1));
			}
		}
		return false;
	}

	private static void listFile(File dir, String tab) throws Exception {
		for (File file : dir.listFiles()) {
			if (file.isFile()) {
				if (file.getName().endsWith("ass")) {
					search(file, tab);
				}
			} else if (file.isDirectory() && !file.isHidden()) {
				listFile(file, "|--" + tab);
			}
		}
	}
}
