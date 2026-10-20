package demo;

import java.lang.reflect.Field;
import java.lang.reflect.Method;

// Provenance for P1Annotated.class — annotated-type and record-reflection additions in P1-b.
// Compiled with `-parameters -g --release 25` (major 69).
public class P1Annotated {
    public String field;

    public void sample(String value) {
    }

    public static String annotated() throws Exception {
        Method method = P1Annotated.class.getDeclaredMethod("sample", String.class);
        Field field = P1Annotated.class.getDeclaredField("field");
        return ((Class<?>) method.getAnnotatedReturnType().getType()).getName()
                + ":" + method.getAnnotatedParameterTypes().length
                + ":" + method.getTypeParameters().length
                + ":" + ((Class<?>) P1Annotated.class.getAnnotatedSuperclass().getType()).getName()
                + ":" + ((Class<?>) field.getAnnotatedType().getType()).getName()
                + ":" + P1Annotated.class.getRecordComponents().length;
    }
}
