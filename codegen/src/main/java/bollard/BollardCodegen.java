package bollard;

import io.swagger.codegen.*;
import io.swagger.codegen.languages.RustServerCodegen;
import io.swagger.models.properties.*;
import io.swagger.models.parameters.Parameter;
import io.swagger.models.parameters.SerializableParameter;
import io.swagger.models.parameters.BodyParameter;
import io.swagger.util.Json;

import io.swagger.models.*;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.apache.commons.lang3.StringUtils;

import java.util.*;
import java.util.Map.Entry;

public class BollardCodegen extends RustServerCodegen {
    private static final Logger LOGGER = LoggerFactory.getLogger(BollardCodegen.class);

    public BollardCodegen() {
        super();
    }

    // Declare custom additions to inline enums that are behaving differently
    // than the official spec
    private static HashMap<String, List<Map<String, String>>> patchEnumValues;
    static {
        patchEnumValues = new HashMap<String, List<Map<String, String>>>();
        Map<String, String> additionalEnumValues = new HashMap<String, String>();
        List<Map<String, String>> enumValues = new ArrayList<Map<String, String>>();

        additionalEnumValues.put("name", "ROLLBACK_STARTED");
        additionalEnumValues.put("value", "\"rollback_started\"");
        enumValues.add(additionalEnumValues);

        additionalEnumValues = new HashMap<String, String>();
        additionalEnumValues.put("name", "ROLLBACK_PAUSED");
        additionalEnumValues.put("value", "\"rollback_paused\"");
        enumValues.add(additionalEnumValues);

        additionalEnumValues = new HashMap<String, String>();
        additionalEnumValues.put("name", "ROLLBACK_COMPLETED");
        additionalEnumValues.put("value", "\"rollback_completed\"");
        enumValues.add(additionalEnumValues);

        patchEnumValues.put("ServiceUpdateStatusStateEnum", enumValues);

        additionalEnumValues = new HashMap<String, String>();
        enumValues = new ArrayList<Map<String, String>>();

        additionalEnumValues.put("name", "NO");
        additionalEnumValues.put("value", "\"no\"");
        enumValues.add(additionalEnumValues);

        patchEnumValues.put("RestartPolicyNameEnum", enumValues);
    }

    private static ArrayList<String> enumToString;
    static {
        enumToString = new ArrayList();
        enumToString.add("HostConfigLogConfig");
    }

    @Override
    public void preprocessSwagger(Swagger swagger) {
        Info info = swagger.getInfo();
        List versionComponents = new ArrayList(Arrays.asList(info.getVersion().split("[.]")));
        while (versionComponents.size() < 3) {
            // Add the package version as a version component to the official specification
            // version
            versionComponents.add((String) additionalProperties.get(CodegenConstants.PACKAGE_VERSION));
        }

        info.setVersion(StringUtils.join(versionComponents, "."));

        super.preprocessSwagger(swagger);
    }

    @Override
    public String getTypeDeclaration(Property p) {
        String type = super.getTypeDeclaration(p);

        // This is a "fallback" type, and allows some parts of the Docker API
        // that receive an empty JSON '{}' value.
        if ("object".equals(type)) {
            type = "HashMap<(), ()>";
        }

        return type;
    }

    @Override
    public CodegenProperty fromProperty(String name, Property p) {
        CodegenProperty property = super.fromProperty(name, p);

        // Remove extraneous references
        if (property.datatype.startsWith("models::")) {
            property.datatype = property.datatype.replace("models::", "");
        }

        return property;
    }

    @Override
    public Map<String, Object> postProcessAllModels(Map<String, Object> objs) {
        Map<String, Object> newObjs = super.postProcessAllModels(objs);

        // Index all CodegenModels by model name.
        HashMap<String, CodegenModel> allModels = new HashMap<String, CodegenModel>();
        for (Entry<String, Object> entry : objs.entrySet()) {
            String modelName = toModelName(entry.getKey());
            Map<String, Object> inner = (Map<String, Object>) entry.getValue();
            List<Map<String, Object>> models = (List<Map<String, Object>>) inner.get("models");
            for (Map<String, Object> mo : models) {
                CodegenModel cm = (CodegenModel) mo.get("model");
                allModels.put(modelName, cm);
            }
        }

        for (Entry<String, CodegenModel> entry : allModels.entrySet()) {
            CodegenModel model = entry.getValue();
            for (CodegenProperty prop : model.vars) {
                if (prop.name.contains("i_pv6")) {
                    prop.name = prop.name.replace("i_pv6", "ipv6");
                } else if (prop.name.contains("i_pv4")) {
                    prop.name = prop.name.replace("i_pv4", "ipv4");
                } else if (prop.name.contains("_i_ops")) {
                    prop.name = prop.name.replace("_i_ops", "_iops");
                } else if (prop.name.contains("_i_ds")) {
                    prop.name = prop.name.replace("_i_ds", "_ids");
                } else if (prop.name.contains("_c_as")) {
                    prop.name = prop.name.replace("_c_as", "_cas");
                } else if (prop.name.equals("_type")) {
                    prop.name = "typ";
                } 
                if (prop.dataFormat != null && prop.dataFormat.equals("dateTime")) {
                    // set DateTime format on properties where appropriate
                    prop.datatype = "DateTime<Utc>";
                }
                if (prop.isEnum) {
                    if (enumToString.contains(model.classname)) {
                        prop.isEnum = false;
                    }
                    ArrayList<HashMap<String, String>> vars = (ArrayList<HashMap<String, String>>) prop.allowableValues
                            .get("enumVars");
                    for (HashMap<String, String> enumVar : vars) {
                        String enumValue = enumVar.get("value");

                        // ensure we can deserialize inline enum values encoded as empty strings
                        if (enumValue != null && enumValue.length() <= 2) {
                            prop.vendorExtensions.put("x-rustgen-has-empty-enum", true);
                        }
                    }

                    // add additional enum values that get patched in at the template level
                    if (patchEnumValues.containsKey(model.classname + prop.enumName)) {
                        prop.vendorExtensions.put("x-rustgen-additional-enum-values",
                                patchEnumValues.get(model.classname + prop.enumName));
                    }
                }
            }
        }

        return newObjs;
    }

    @Override
    public void postProcessModelProperty(CodegenModel model, CodegenProperty property) {
        super.postProcessModelProperty(model, property);

        if (property.datatype.equals("isize")) {
            // needed for windows
            property.datatype = "i64";
        }
    }

    @Override
    public String toEnumVarName(String value, String datatype) {
        String name = super.toEnumVarName(value, datatype);
        if (name.length() == 0) {
            return "EMPTY";
        }
        return name;
    }
}
