/**
 * 表单校验工具：基于浏览器原生 Constraint Validation API。
 * 不重复造校验引擎，只做属性配置 + 统一收集/报错，输入数据绝不注入 DOM 结构。
 */

const PATTERNS = {
  phone: '^\\+?[0-9\\-\\s]{5,20}$',
  imei: '^[0-9]{15}$',
  port: '^[0-9]{1,5}$',
  ipv4: '^([0-9]{1,3}\\.){3}[0-9]{1,3}$',
  number: '^-?[0-9]+(\\.[0-9]+)?$',
  hostname: '^[A-Za-z0-9._:-]{1,253}$',
  apn: '^[A-Za-z0-9._-]{1,100}$'
};

/** 返回可写入 input[pattern] 的字符串。 */
export function patternOf(name) {
  return PATTERNS[name] || undefined;
}

/**
 * 为 input 配置原生约束。rules: { required, pattern, min, max, minlength,
 * maxlength, step, type, message }。message 作为自定义校验提示。
 */
export function constrain(input, rules = {}) {
  if (!input) return input;
  if (rules.type) input.type = rules.type;
  if (rules.required) input.required = true;
  if (rules.pattern) input.pattern = rules.pattern;
  if (rules.min !== undefined) input.min = String(rules.min);
  if (rules.max !== undefined) input.max = String(rules.max);
  if (rules.step !== undefined) input.step = String(rules.step);
  if (rules.minlength !== undefined) input.minLength = rules.minlength;
  if (rules.maxlength !== undefined) input.maxLength = rules.maxlength;
  if (rules.placeholder) input.placeholder = rules.placeholder;
  const message = rules.message || '';
  const refresh = () => {
    // 先清除再依据当前 value 设置，保证 reportValidity 文案正确
    input.setCustomValidity('');
    if (input.value && rules.pattern) {
      const re = new RegExp(`^(?:${rules.pattern})$`);
      if (!re.test(input.value)) input.setCustomValidity(message || '格式不正确');
    }
  };
  input.addEventListener('input', refresh);
  input.addEventListener('change', refresh);
  return input;
}

/**
 * 校验整个 form：使用 Constraint Validation API。
 * 返回 { ok, values }；不 ok 时调用 reportValidity() 高亮首个错误字段。
 */
export function validateForm(form) {
  if (!form) return { ok: false, values: {} };
  const ok = typeof form.reportValidity === 'function'
    ? form.reportValidity()
    : form.checkValidity();
  const values = {};
  if (ok && typeof FormData !== 'undefined') {
    const data = new FormData(form);
    for (const [key, value] of data.entries()) {
      values[key] = value;
    }
  }
  return { ok, values };
}

/** 端口范围校验（1-65535），供业务层补充数值边界判断。 */
export function isPortValid(value) {
  const n = Number(value);
  return Number.isInteger(n) && n >= 1 && n <= 65535;
}

/** IMEI 简单校验（15 位数字）。 */
export function isImei(value) {
  return /^[0-9]{15}$/.test(String(value || ''));
}

/** 手机号校验。 */
export function isPhone(value) {
  return new RegExp(`^(?:${PATTERNS.phone})$`).test(String(value || '').trim());
}

/** 两个密码是否一致。 */
export function passwordsMatch(a, b) {
  return typeof a === 'string' && a.length >= 4 && a === b;
}
