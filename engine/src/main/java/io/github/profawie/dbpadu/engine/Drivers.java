package io.github.profawie.dbpadu.engine;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.sql.Connection;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.util.Map;
import java.util.Properties;
import java.util.ServiceLoader;
import java.util.concurrent.ConcurrentHashMap;
import java.util.jar.JarFile;
import java.util.jar.Manifest;

/**
 * The JDBC drivers: the ones inside the engine, and drivers the user
 * points to in a jar file of their own.
 */
final class Drivers {
    private Drivers() {}

    /** A driver from the user's own jar, loaded once and kept. */
    private record Loaded(Driver driver, String version) {}

    private static final Map<String, Loaded> OWN = new ConcurrentHashMap<>();

    /** Name, package, class and version of every driver inside the engine. */
    static JsonArray bundled() {
        Properties properties = new Properties();
        try (InputStream in = Drivers.class.getResourceAsStream("/drivers.properties")) {
            if (in != null) {
                properties.load(in);
            }
        } catch (IOException unreadable) {
            // The list stays empty; the drivers themselves still work.
        }
        JsonArray drivers = new JsonArray();
        for (String kind : properties.getProperty("kinds", "").split(",")) {
            if (kind.isBlank()) {
                continue;
            }
            JsonObject driver = new JsonObject();
            driver.addProperty("kind", kind);
            driver.addProperty("name", properties.getProperty(kind + ".name", ""));
            driver.addProperty("package", properties.getProperty(kind + ".package", ""));
            driver.addProperty("className", properties.getProperty(kind + ".class", ""));
            driver.addProperty("version", properties.getProperty(kind + ".version", ""));
            drivers.add(driver);
        }
        return drivers;
    }

    /**
     * Open a connection. With {@code jar}, the driver comes from that file
     * instead of the engine; {@code className} may be empty to use the
     * driver the jar announces.
     */
    static Connection connect(String url, Properties properties, String jar, String className)
            throws SQLException {
        if (jar == null || jar.isBlank()) {
            return DriverManager.getConnection(url, properties);
        }
        Driver driver = own(jar, className).driver();
        Connection connection = driver.connect(url, properties);
        if (connection == null) {
            throw new SQLException(
                "The driver in " + jar + " does not understand the address " + url);
        }
        return connection;
    }

    /** What is in the user's jar: the driver class and its version. */
    static JsonObject describe(String jar, String className) throws SQLException {
        Loaded loaded = own(jar, className);
        JsonObject result = new JsonObject();
        result.addProperty("className", loaded.driver().getClass().getName());
        result.addProperty("version", loaded.version());
        return result;
    }

    private static Loaded own(String jar, String className) throws SQLException {
        File file = new File(jar);
        if (!file.isFile()) {
            throw new SQLException("The driver file was not found: " + jar);
        }
        String wanted = className == null ? "" : className.trim();
        // A replaced file is a new driver.
        String key = file.getAbsolutePath() + "|" + file.lastModified() + "|" + wanted;
        Loaded known = OWN.get(key);
        if (known != null) {
            return known;
        }
        try {
            // Only Java itself is shared with the jar, so its classes win
            // over the driver of the same name inside the engine.
            URLClassLoader loader = new URLClassLoader(
                new URL[] {file.toURI().toURL()}, ClassLoader.getPlatformClassLoader());
            Driver driver = null;
            if (wanted.isEmpty()) {
                for (Driver found : ServiceLoader.load(Driver.class, loader)) {
                    if (found.getClass().getClassLoader() == loader) {
                        driver = found;
                        break;
                    }
                }
                if (driver == null) {
                    throw new SQLException(
                        "No JDBC driver was found in " + jar + ". Enter the driver class.");
                }
            } else {
                driver = (Driver) Class.forName(wanted, true, loader)
                    .getDeclaredConstructor()
                    .newInstance();
            }
            Loaded loaded = new Loaded(driver, version(file, driver));
            OWN.put(key, loaded);
            return loaded;
        } catch (SQLException problem) {
            throw problem;
        } catch (ClassNotFoundException missing) {
            throw new SQLException("The class " + wanted + " is not in " + jar);
        } catch (ReflectiveOperationException | IOException | LinkageError | ClassCastException problem) {
            throw new SQLException("The driver in " + jar + " could not be loaded: " + problem);
        }
    }

    /** The version the jar states, else the driver's own major.minor. */
    private static String version(File file, Driver driver) {
        try (JarFile jar = new JarFile(file)) {
            Manifest manifest = jar.getManifest();
            if (manifest != null) {
                for (String name : new String[] {"Implementation-Version", "Bundle-Version"}) {
                    String value = manifest.getMainAttributes().getValue(name);
                    if (value != null && !value.isBlank()) {
                        return value.trim();
                    }
                }
            }
        } catch (IOException unreadable) {
            // Fall through to what the driver says.
        }
        return driver.getMajorVersion() + "." + driver.getMinorVersion();
    }
}
