package demo;

import java.beans.BeanInfo;
import java.beans.Introspector;
import java.beans.PropertyChangeEvent;
import java.beans.PropertyChangeListener;
import java.beans.PropertyChangeSupport;
import java.beans.PropertyDescriptor;
import java.beans.PropertyEditor;
import java.beans.PropertyEditorManager;

// Provenance for P1Beans.class — java.beans introspection and editor APIs in P1-b.
// Compiled with `-parameters -g --release 25` (major 69).
public class P1Beans {
    public static class Bean {
        private String name;
        private int age;

        public String getName() {
            return name;
        }

        public void setName(String name) {
            this.name = name;
        }

        public int getAge() {
            return age;
        }

        public void setAge(int age) {
            this.age = age;
        }
    }

    public static String methods() {
        StringBuilder builder = new StringBuilder();
        for (java.lang.reflect.Method method : Bean.class.getDeclaredMethods()) {
            builder.append(method.getName()).append(',');
        }
        return builder.toString();
    }

    public static String descriptor() throws Exception {
        PropertyDescriptor descriptor = new PropertyDescriptor("name", Bean.class);
        return descriptor.getName() + ":" + descriptor.getPropertyType().getName()
                + ":" + (descriptor.getReadMethod() != null)
                + ":" + (descriptor.getWriteMethod() != null);
    }

    public static String introspector() throws Exception {
        BeanInfo info = Introspector.getBeanInfo(Bean.class);
        PropertyDescriptor[] properties = info.getPropertyDescriptors();
        boolean name = false;
        boolean age = false;
        for (PropertyDescriptor property : properties) {
            name |= property.getName().equals("name");
            age |= property.getName().equals("age");
        }
        return properties.length + ":" + name + ":" + age + ":"
                + info.getBeanDescriptor().getBeanClass().getName();
    }

    public static String editor() {
        PropertyEditor editor = PropertyEditorManager.findEditor(Integer.class);
        editor.setAsText("42");
        return editor.getAsText();
    }

    public static String changeSupport() {
        PropertyChangeSupport support = new PropertyChangeSupport(new Object());
        final StringBuilder events = new StringBuilder();
        support.addPropertyChangeListener(new PropertyChangeListener() {
            public void propertyChange(PropertyChangeEvent event) {
                events.append(event.getPropertyName()).append(':')
                        .append(event.getOldValue()).append(':')
                        .append(event.getNewValue());
            }
        });
        support.firePropertyChange("name", "a", "b");
        return events.toString();
    }
}
