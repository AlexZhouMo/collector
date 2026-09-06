import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.util.Arrays;

public class ComicAdjust {
	private static final boolean FLAG = true;
	private static final String KEY = "S04E";
	private static final String SUBS = "\\N{\\fnÎ¢ÈíÑÅºÚ\\fs14}";
	private static final String ENTER = "\r\n";

	public static void main(String[] args) throws Exception {
		for (File file : new File("G:\\\\ÐÐÊ¬×ßÈâ").listFiles()) {
			if (file.getName().endsWith("ass")) {
				String inputCharset = FLAG ? "UTF-8" : "Unicode";
				InputStream inputStream = new FileInputStream(file.getAbsolutePath());
				InputStreamReader inputStreamReader = new InputStreamReader(inputStream, inputCharset);
				BufferedReader bufferedReader = new BufferedReader(inputStreamReader);
				BufferedWriter bufferedWriter = new BufferedWriter(new OutputStreamWriter(new FileOutputStream(file.getAbsolutePath().replace(KEY, "_" + KEY)), "UTF-8"));
				String line;
				while ((line = bufferedReader.readLine()) != null) {
					line = line.replace(SUBS, "\\N{\\fnArial\\fs30}");
					bufferedWriter.write(line + ENTER);
				}
				inputStream.close();
				inputStreamReader.close();
				bufferedReader.close();
				bufferedWriter.close();
				System.out.println(file.getName() + " is done.");
			}
			if (file.isDirectory()) {
				String[] files = file.list();
				Arrays.sort(files);
				String path = file.getPath() + "\\";
				for (String fileName : files) {
					if (fileName.contains("-_000.jpg")) {
						System.out.println(fileName);
						new File(path + fileName).renameTo(new File(path + fileName.replace("-_000.jpg", "-_0000a.jpg")));
					}
					if (fileName.length() == 6) {
						System.out.println("0" + fileName);
						new File(path + fileName).renameTo(new File(path + "00" + fileName));
					}
				}
			}
		}
	}
}
