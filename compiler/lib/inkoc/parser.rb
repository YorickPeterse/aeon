# frozen_string_literal: true

module Inkoc
  # rubocop: disable Metrics/ClassLength
  class Parser
    ParseError = Class.new(StandardError)

    MESSAGE_TOKENS = Set.new(
      %i[
        add
        and
        as
        bitwise_and
        bitwise_or
        bitwise_xor
        bracket_open
        constant
        div
        else
        equal
        exclusive_range
        greater
        greater_equal
        identifier
        impl
        import
        inclusive_range
        let
        lower
        lower_equal
        mod
        mul
        not_equal
        class
        or
        pow
        return
        self
        shift_left
        shift_right
        sub
        throw
        trait
        mut
        for
        impl
        try
        do
        lambda
        match
        match_equal
        when
        yield
        ref
        move
        if
        and
        or
        not
        in
        integer
        loop
        next
        for
        while
      ]
    ).freeze

    VALUE_START = Set.new(
      %i[
        attribute
        bracket_open
        constant
        curly_open
        define
        do
        float
        identifier
        impl
        integer
        lambda
        let
        let
        paren_open
        return
        self
        string
        throw
        trait
        try
        try_bang
        match
        yield
        tstring_open
        if
        for
        loop
        while
        not
      ]
    ).freeze

    BINARY_OPERATORS = Set.new(
      %i[
        equal
        not_equal
        lower
        lower_equal
        greater
        greater_equal
        bitwise_or
        bitwise_xor
        bitwise_and
        shift_left
        shift_right
        add
        sub
        div
        mod
        mul
        pow
        inclusive_range
        exclusive_range
        match_equal
      ]
    ).freeze

    BINARY_REASSIGN_OPERATORS = Set.new(
      %i[
        div_assign
        mod_assign
        bitwise_xor_assign
        bitwise_and_assign
        bitwise_or_assign
        pow_assign
        mul_assign
        sub_assign
        add_assign
        shift_left_assign
        shift_right_assign
      ]
    ).freeze

    def initialize(input, file_path = Pathname.new('(eval)'), parse_comments: false)
      @lexer = Lexer.new(input, file_path, parse_comments: parse_comments)
      @without_trailing_block = 0
    end

    def without_trailing_block
      @without_trailing_block += 1
      retval = yield
      @without_trailing_block -= 1

      retval
    end

    def comments
      @lexer.comments
    end

    def location
      @lexer.current_location
    end

    def line
      @lexer.line
    end

    def column
      @lexer.column
    end

    def parse
      expressions
    end

    def expressions
      location = @lexer.current_location
      children = []

      while (token = @lexer.advance) && token.valid?
        children << top_level(token)
      end

      AST::Body.new(children, location)
    end

    # rubocop: disable Metrics/CyclomaticComplexity
    def top_level(start)
      case start.type
      when :import
        import(start)
      when :class
        def_class(start)
      when :trait
        def_trait(start)
      when :impl
        implement_trait(start)
      when :define
        def_method(start)
      when :extern
        def_extern_method(start)
      else
        expression(start)
      end
    end
    # rubocop: enable Metrics/CyclomaticComplexity

    # Parses an import statement.
    #
    # Examples:
    #
    #     import foo
    #     import foo::bar
    #     import foo::bar::(Baz as Bla)
    def import(start)
      steps = []
      symbols = []
      step = advance_and_expect!(:identifier)

      loop do
        case step.type
        when :identifier, :class, :trait, :return, :loop
          steps << identifier_from_token(step)
        when :constant
          symbol = import_symbol_from_token(step)
          symbols << AST::ImportSymbol.new(symbol, nil, step.location)
          break
        when :mul
          symbols << AST::GlobImport.new(step.location)
          break
        else
          raise ParseError, "#{step.type} is not valid in import statements"
        end

        break unless @lexer.next_type_is?(:colon_colon)

        skip_one

        if @lexer.next_type_is?(:paren_open)
          skip_one
          symbols = import_symbols
          break
        end

        step = advance!
      end

      AST::Import.new(steps, symbols, start.location)
    end

    def import_symbols
      symbols = []

      loop do
        start = advance!
        symbol = import_symbol_from_token(start)

        alias_name =
          if @lexer.next_type_is?(:as)
            skip_one
            import_alias_from_token(advance!)
          end

        symbols << AST::ImportSymbol.new(symbol, alias_name, start.location)

        break if comma_or_break_on(:paren_close)
      end

      symbols
    end

    def import_symbol_from_token(start)
      case start.type
      when :identifier, :constant
        identifier_from_token(start)
      when :self
        self_object(start)
      else
        raise(
          ParseError,
          "#{start.type.inspect} is not a valid import symbol"
        )
      end
    end

    def import_alias_from_token(start)
      case start.type
      when :identifier, :constant
        identifier_from_token(start)
      else
        raise(
          ParseError,
          "#{start.type.inspect} is not a valid symbol alias"
        )
      end
    end

    def expression(start)
      type_cast(start)
    end

    def type_cast(start)
      node = boolean_keywords(start)

      while @lexer.next_type_is?(:as)
        advance!

        type = type(advance!)
        node = AST::TypeCast.new(node, type, start.location)
      end

      node
    end

    def boolean_keywords(start)
      node = binary_send(start)

      loop do
        case @lexer.peek.type
        when :and
          op = advance!
          rhs = binary_send(advance!)
          node = AST::And.new(node, rhs, op.location)
        when :or
          op = advance!
          rhs = binary_send(advance!)
          node = AST::Or.new(node, rhs, op.location)
        else
          break
        end
      end

      node
    end

    def binary_send(start)
      node = send_chain(start)

      while BINARY_OPERATORS.include?(@lexer.peek.type)
        operator = @lexer.advance
        rhs = send_chain(@lexer.advance)
        node = AST::Send.new(operator.value, node, [], [rhs], operator.location)
      end

      node
    end

    # Parses an expression such as `[X]` or `[X] = Y`.
    def bracket_get_or_set
      args = []

      while (token = @lexer.advance) && token.valid_but_not?(:bracket_close)
        args << expression(token)

        if @lexer.next_type_is?(:comma)
          @lexer.advance
          next
        end

        next if @lexer.peek.type == :bracket_close

        raise(
          ParseError,
          "Expected a closing bracket, got #{@lexer.peek.type.inspect} instead"
        )
      end

      name = if @lexer.next_type_is?(:assign)
               args << expression(skip_and_advance!)

               '[]='
             else
               '[]'
             end

      [name, args]
    end

    # Parses a type name.
    #
    # Examples:
    #
    #     Foo
    #     Foo!(Bar)
    #     Foo::Bar!(Baz)
    def type_name(token)
      AST::TypeName
        .new(constant(token), optional_type_parameters, token.location)
    end

    # Parses a block type.
    #
    # Examples:
    #
    #     do
    #     do (A)
    #     do (A, B)
    #     do (A) -> R
    #     do (A) !! X -> R
    def block_type(start, type = :do)
      args = block_type_arguments
      throws = optional_throw_type
      returns = optional_return_type
      klass = type == :lambda ? AST::LambdaType : AST::BlockType

      klass.new(args, returns, throws, start.location)
    end

    def moving_block_type(start)
      advance_and_expect!(:do)

      node = block_type(start)
      node.moving = true

      node
    end

    def block_type_arguments
      args = []

      if @lexer.next_type_is?(:paren_open)
        skip_one

        while (token = @lexer.advance) && token.valid_but_not?(:paren_close)
          args << type(token)

          break if comma_or_break_on(:paren_close)
        end
      end

      args
    end

    # Parses a type argument.
    def def_type_parameter(token)
      node = AST::DefineTypeParameter.new(token.value, token.location)

      if @lexer.next_type_is?(:colon)
        skip_one

        node.required_traits = required_traits
      end

      node
    end

    # Parses a chain of messages being sent to a receiver.
    def send_chain(start)
      node = value(start)

      loop do
        case @lexer.peek.type
        when :dot
          skip_one
          node = send_chain_with_receiver(node)
        when :bracket_open
          # Only treat [x][y] as a send if [y] occurs on the same line. This
          # ensures that e.g. [x]\n[y] is parsed as two array literals.
          break unless @lexer.peek.line == start.line

          bracket = @lexer.advance
          name, args = bracket_get_or_set

          node = AST::Send.new(name, node, [], args, bracket.location)
        when :exclamation
          raise ParseError, 'Unexpected !'
        else
          break
        end
      end

      node
    end

    def send_chain_with_receiver(receiver)
      name_start = advance!
      name = message_name_for_token(name_start)
      location = name_start.location
      args = []
      peeked = @lexer.peek

      if peeked.type == :type_args_open
        skip_one

        type_args = type_parameters
        peeked = @lexer.peek
      else
        type_args = []
      end

      if peeked.type == :paren_open && peeked.line == location.line
        args = arguments_with_parenthesis
      elsif next_expression_is_argument?(name_start)
        return send_without_parenthesis(receiver, name, type_args, location)
      end

      AST::Send.new(name, receiver, type_args, args, location)
    end

    # Returns true if the next expression is an argument to use when parsing
    # arguments without parenthesis.
    def next_expression_is_argument?(current)
      peeked = @lexer.peek
      current_end = current.value.length + current.column

      if peeked.type == :curly_open && @without_trailing_block.positive?
        # Trailing blocks aren't allowed in conditions, as this makes it
        # impossible to parse them without requiring parentheses.
        return false
      end

      # Something is only an argument if:
      #
      # 1. It resides on the same line.
      # 2. It is separated by at least a single space.
      VALUE_START.include?(peeked.type) &&
        peeked.line == current.line &&
        (peeked.column - current_end) >= 1
    end

    # Parses a list of send arguments wrapped in parenthesis.
    #
    # Example:
    #
    #     (10, 'foo', 'bar')
    #     (10, 'foo', 'bar') do { ... }
    # rubocop: disable Metrics/CyclomaticComplexity
    # rubocop: disable Metrics/PerceivedComplexity
    def arguments_with_parenthesis
      args = []
      paren_line = nil

      # Skip the opening parenthesis
      skip_one

      while (token = @lexer.advance) && token.valid?
        if token.type == :paren_close
          paren_line = token.line
          break
        end

        args << expression_or_keyword_argument(token)

        if @lexer.next_type_is?(:comma)
          skip_one
        elsif @lexer.peek.valid_but_not?(:paren_close)
          raise ParseError, "Expected a comma, not #{@lexer.peek.value.inspect}"
        end
      end

      # If a block follows the send on the same line as the closing parenthesis,
      # we include it as the last argument.
      if (trailing_block = trailing_block_for_send(paren_line))
        args << trailing_block
      end

      args
    end
    # rubocop: enable Metrics/PerceivedComplexity
    # rubocop: enable Metrics/CyclomaticComplexity

    def trailing_block_for_send(paren_line)
      return unless @lexer.peek.line == paren_line

      case @lexer.peek.type
      when :curly_open
        return if @without_trailing_block.positive?

        block_without_arguments(advance!)
      when :do, :lambda
        token = advance!

        block(token, token.type)
      end
    end

    # Parses a list of send arguments without parenthesis.
    #
    # Example:
    #
    #     foo 10, 'foo', 'bar'
    def send_without_parenthesis(receiver, name, type_arguments, location)
      args = []

      while (token = @lexer.advance) && token.valid?
        arg, is_block = argument_for_send_without_parenthesis(token)
        args << arg

        if is_block && @lexer.next_type_is?(:dot)
          skip_one

          node = AST::Send.new(name, receiver, type_arguments, args, location)

          return send_chain_with_receiver(node)
        end

        break unless @lexer.next_type_is?(:comma)

        skip_one
      end

      AST::Send.new(name, receiver, type_arguments, args, location)
    end

    def argument_for_send_without_parenthesis(token)
      case token.type
      when :curly_open
        [block_without_arguments(token), true]
      when :do
        [block(token), true]
      when :lambda
        [block(token, :lambda), true]
      else
        [expression_or_keyword_argument(token), false]
      end
    end

    def expression_or_keyword_argument(start)
      if @lexer.next_type_is?(:colon)
        skip_one

        value = expression(advance!)

        AST::KeywordArgument.new(start.value, value, start.location)
      else
        expression(start)
      end
    end

    # rubocop: disable Metrics/AbcSize
    # rubocop: disable Metrics/CyclomaticComplexity
    def value(start)
      case start.type
      when :string then string(start)
      when :integer then integer(start)
      when :float then float(start)
      when :identifier then identifier_or_reassign(start)
      when :constant then constant_value(start)
      when :curly_open then block_without_arguments(start)
      when :define then def_method(start)
      when :static then def_static_method(start)
      when :do, :lambda then block(start, start.type)
      when :let then let_define(start)
      when :return then return_value(start)
      when :attribute then attribute_or_reassign(start)
      when :self then self_object(start)
      when :throw then throw_value(start)
      when :try then try(start)
      when :try_bang then try_bang(start)
      when :colon_colon then global(start)
      when :paren_open then grouped_expression
      when :match then pattern_match(start)
      when :if then if_expression(start)
      when :yield then yield_value(start)
      when :tstring_open then template_string(start)
      when :ref then ref_value(start)
      when :not then not_value(start)
      when :for then for_loop(start)
      when :loop then infinite_loop(start)
      when :next then loop_next(start)
      when :break then loop_break(start)
      when :while then while_loop(start)
      when :move then moving_block(start)
      else
        raise ParseError, "A value can not start with a #{start.type.inspect}"
      end
    end
    # rubocop: enable Metrics/AbcSize
    # rubocop: enable Metrics/CyclomaticComplexity

    def ref_value(start)
      AST::ReferenceValue.new(expression(advance!), start.location)
    end

    def not_value(start)
      # We use binary_send() here so `not a or b` is parsed as `(not a) or (b)`,
      # instead of `not(a or b)`.
      AST::Not.new(binary_send(advance!), start.location)
    end

    def for_loop(start)
      binding = without_trailing_block { for_loop_binding }

      advance_and_expect!(:in)

      error =
        case @lexer.peek.type
        when :try, :try_bang
          advance!.type
        end

      iterable = without_trailing_block { expression(advance!) }
      body = block_body(advance_and_expect!(:curly_open))
      else_arg = nil
      else_body = nil

      if error == :try && @lexer.next_type_is?(:else)
        skip_one

        else_arg = optional_else_arg
        else_body = block_with_optional_curly_braces
      end

      AST::For
        .new(binding, error, iterable, body, else_arg, else_body, start.location)
    end

    def for_loop_binding
      type = @lexer.peek.type

      case type
      when :identifier, :mut
        for_loop_single_binding
      when :paren_open
        for_loop_destructure
      else
        raise ParseError, "Unexpected token of type #{type}"
      end
    end

    def for_loop_single_binding
      mutable = next_if_mutable
      name = variable_name
      vtype = optional_variable_type

      AST::ForBinding.new(name, mutable, vtype, name.location)
    end

    def for_loop_destructure
      start = advance!
      vars = destructure_array_variables

      AST::ForDestructure.new(vars, start.location)
    end

    def infinite_loop(start)
      body = block_body(advance_and_expect!(:curly_open))

      AST::Loop.new(body, start.location)
    end

    def while_loop(start)
      cond = without_trailing_block { expression(advance!) }
      body = block_body(advance_and_expect!(:curly_open))

      AST::While.new(cond, body, start.location)
    end

    def loop_next(start)
      AST::Next.new(start.location)
    end

    def loop_break(start)
      AST::Break.new(start.location)
    end

    def string(start)
      AST::String.new(start.value, start.location)
    end

    def template_string(start)
      members = []

      while (token = @lexer.advance) && token.valid_but_not?(:tstring_close)
        if token.type == :tstring_expr_open
          unless @lexer.next_type_is?(:tstring_expr_close)
            members << expression(advance!)
          end

          advance_and_expect!(:tstring_expr_close)
        else
          members << string(token)
        end
      end

      AST::TemplateString.new(members, start.location)
    end

    def integer(start)
      AST::Integer.new(Integer(start.value), start.location)
    end

    def unsigned_integer(start)
      AST::UnsignedInteger.new(Integer(start.value), start.location)
    end

    def float(start)
      AST::Float.new(Float(start.value), start.location)
    end

    def identifier_or_reassign(start)
      return reassign_local(start) if @lexer.next_type_is?(:assign)

      node = identifier(start)

      if next_is_binary_reassignment?
        reassign_binary(node)
      else
        node
      end
    end

    def identifier(start)
      peeked = @lexer.peek

      if peeked.type == :type_args_open
        skip_one

        type_args = type_parameters
        peeked = @lexer.peek
      else
        type_args = []
      end

      if peeked.type == :paren_open && peeked.line == start.line
        args = arguments_with_parenthesis

        AST::Send.new(start.value, nil, type_args, args, start.location)
      elsif next_expression_is_argument?(start)
        # If an identifier is followed by another expression on the same line
        # we'll treat said expression as the start of an argument list.
        send_without_parenthesis(nil, start.value, type_args, start.location)
      else
        identifier_from_token(start, type_args)
      end
    end

    # Parses a constant.
    #
    # Examples:
    #
    #     Foo
    #     Foo::Bar
    def constant(start)
      constant_from_token(start)
    end

    def constant_value(start)
      peeked = @lexer.peek

      if peeked.type == :curly_open &&
          peeked.line == start.line &&
          @without_trailing_block.zero?
        new_instance(start)
      else
        constant_from_token(start)
      end
    end

    def new_instance(start)
      # Skip the opening {
      advance!

      attrs = []

      loop do
        if @lexer.next_type_is?(:curly_close)
          advance!
          break
        end

        name = advance_and_expect!(:attribute)

        advance_and_expect!(:assign)

        value = expression(advance!)

        attrs << AST::AssignAttribute.new(name.value, value, name.location)

        break if comma_or_break_on(:curly_close)
      end

      AST::NewInstance.new(start.value, attrs, start.location)
    end

    # Parses a reference to a module global.
    #
    # Example:
    #
    #     ::Foo
    def global(start)
      token = advance!

      name =
        if token.type == :identifier || token.type == :constant
          token.value
        else
          raise(
            ParseError,
            "Unexpected #{token.type}, expected an identifier or constant"
          )
        end

      AST::Global.new(name, start.location)
    end

    # Parses a grouped expression.
    #
    # Example:
    #
    #   (10 + 20)
    def grouped_expression
      expr = expression(advance!)

      advance_and_expect!(:paren_close)

      expr
    end

    # Parses a block without arguments.
    #
    # Examples:
    #
    #     { body }
    def block_without_arguments(start)
      loc = start.location

      AST::Block.new([], [], nil, nil, block_body(start), loc, signature: false)
    end

    # Parses a block starting with the "do" keyword.
    #
    # Examples:
    #
    #     do { body }
    #     do (arg) { body }
    #     do (arg: T) { body }
    #     do (arg: T) -> T { body }
    def block(start, type = :do)
      targs = optional_type_parameter_definitions
      args = optional_arguments
      throw_type = optional_throw_type
      ret_type = optional_return_type
      body = block_body(advance_and_expect!(:curly_open))
      klass = type == :lambda ? AST::Lambda : AST::Block

      klass.new(targs, args, ret_type, throw_type, body, start.location)
    end

    def moving_block(start)
      advance_and_expect!(:do)

      node = block(start)
      node.moving = true

      node
    end

    # Parses the body of a block.
    def block_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        nodes << expression(token)
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses a method definition.
    #
    # Examples:
    #
    #     def foo { ... }
    #     def foo
    #     def foo -> A { ... }
    #     def foo!(T)(arg: T) -> T { ... }
    #     def foo !! B -> A { ... }
    #     def foo !! B -> A where A: B { ... }
    def def_method(start)
      name_token = advance!
      name = message_name_for_token(name_token)
      targs = optional_type_parameter_definitions
      arguments = optional_arguments
      throw_type = optional_throw_type
      ret_type = optional_return_type
      yield_type = optional_yield_type
      required = false
      requirements = optional_method_requirements

      body =
        if @lexer.next_type_is?(:curly_open)
          block_body(advance!)
        else
          required = true
          AST::Body.new([], start.location)
        end

      AST::Method.new(
        name,
        arguments,
        targs,
        ret_type,
        yield_type,
        throw_type,
        required,
        requirements,
        body,
        start.location
      )
    end

    def def_static_method(start)
      def_start = advance_and_expect!(:define)

      def_method(def_start).tap do |method|
        method.static = true
      end
    end

    def def_extern_method(start)
      advance_and_expect!(:define)

      name_token = advance!
      name = message_name_for_token(name_token)
      arguments = optional_arguments
      throw_type = optional_throw_type
      ret_type = optional_return_type

      AST::ExternMethod
        .new(name, arguments, ret_type, throw_type, start.location)
    end

    def def_move_method(start)
      def_start = advance_and_expect!(:define)

      def_method(def_start).tap do |method|
        method.move = true
      end
    end

    # Parses a list of argument definitions.
    # rubocop: disable Metrics/CyclomaticComplexity
    def def_arguments
      args = []

      while (token = advance!) && token.valid_but_not?(:paren_close)
        token, rest = advance_if_rest_argument(token)

        if token.type != :identifier
          raise(ParseError, "Expected an identifier, not #{token.type}")
        end

        type = optional_argument_type

        args << AST::DefineArgument.new(token.value, type, rest, token.location)

        break if comma_or_break_on(:paren_close) || rest
      end

      args
    end
    # rubocop: enable Metrics/CyclomaticComplexity

    def advance_if_rest_argument(token)
      if token.type == :mul
        [advance!, true]
      else
        [token, false]
      end
    end

    def optional_argument_type
      return unless @lexer.next_type_is?(:colon)

      skip_one

      type(advance!)
    end

    # Parses a list of type argument definitions.
    def def_type_parameters
      args = []

      while @lexer.peek.valid_but_not?(:paren_close)
        args << def_type_parameter(advance_and_expect!(:constant))

        break if comma_or_break_on(:paren_close)
      end

      args
    end

    def type_parameters
      args = []

      while @lexer.peek.valid_but_not?(:paren_close)
        args << type(advance!)

        break if comma_or_break_on(:paren_close)
      end

      args
    end

    def optional_arguments
      if @lexer.next_type_is?(:paren_open)
        skip_one
        def_arguments
      else
        []
      end
    end

    def optional_type_parameter_definitions
      if @lexer.next_type_is?(:type_args_open)
        skip_one
        def_type_parameters
      else
        []
      end
    end

    def optional_type_parameters
      if @lexer.next_type_is?(:type_args_open)
        skip_one
        type_parameters
      else
        []
      end
    end

    def optional_return_type
      return unless @lexer.next_type_is?(:arrow)

      skip_one

      type(advance!)
    end

    def optional_yield_type
      return unless @lexer.next_type_is?(:darrow)

      skip_one
      type(advance!)
    end

    def optional_throw_type
      return unless @lexer.next_type_is?(:throws)

      skip_one

      type(advance!)
    end

    def optional_method_requirements
      return [] unless @lexer.next_type_is?(:when)

      skip_one

      method_requirements
    end

    def method_requirements
      requirements = []

      while @lexer.next_type_is?(:constant)
        param = advance_and_expect!(:constant)

        advance_and_expect!(:colon)

        requirements << AST::MethodRequirement
          .new(param.value, required_traits, param.location)

        break unless @lexer.next_type_is?(:comma)

        skip_one
      end

      requirements
    end

    def required_traits
      required = []

      loop do
        required << type(advance!)

        break unless @lexer.next_type_is?(:add)

        skip_one
      end

      required
    end

    # Parses a definition of a variable.
    #
    # Example:
    #
    #     let number = 10
    #     let mut number = 10
    def let_define(start)
      if @lexer.next_type_is?(:paren_open)
        return destructure_array(start)
      end

      mutable = next_if_mutable
      name = variable_name
      vtype = optional_variable_type
      value = variable_value

      AST::DefineVariable.new(name, value, vtype, mutable, start.location)
    end

    def next_if_mutable
      if @lexer.next_type_is?(:mut)
        skip_one
        true
      else
        false
      end
    end

    def destructure_array(start)
      advance!

      vars = destructure_array_variables

      advance_and_expect!(:assign)

      AST::DestructArray.new(vars, expression(advance!), start.location)
    end

    def destructure_array_variables
      vars = []

      loop do
        token = @lexer.advance
        loc = token.location

        break if token.type == :paren_close || token.nil?

        mutable =
          if token.type == :mut
            token = @lexer.advance
            true
          else
            false
          end

        unless token.type == :identifier
          raise ParseError, "A #{token.type.inspect} is not valid here"
        end

        name = identifier_from_token(token)
        vtype = optional_variable_type

        vars << AST::DestructArrayVariable.new(name, vtype, mutable, loc)

        break if comma_or_break_on(:paren_close)
      end

      vars
    end

    def def_attribute(start)
      advance_and_expect!(:colon)

      vtype = type(advance!)

      AST::DefineAttribute.new(start.value, vtype, start.location)
    end

    # Parses the name of a variable definition.
    def variable_name
      start = advance!

      case start.type
      when :identifier then identifier_from_token(start)
      when :constant then constant_from_token(start)
      else
        raise(
          ParseError,
          "Unexpected #{start.type}, expected an identifier or constant "
        )
      end
    end

    # Parses the optional definition of a variable type.
    #
    # Example:
    #
    #     let x: Integer = 10
    def optional_variable_type
      return unless @lexer.next_type_is?(:colon)

      skip_one
      type(advance!)
    end

    def variable_value
      advance_and_expect!(:assign)
      expression(advance!)
    end

    # Parses a class definition.
    def def_class(start)
      name = advance_and_expect!(:constant)
      targs = optional_type_parameter_definitions
      body = class_body(advance_and_expect!(:curly_open))

      AST::Object.new(name.value, targs, body, start.location)
    end

    # Parses the body of a class definition.
    def class_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        node =
          case token.type
          when :define then def_method(token)
          when :move then def_move_method(token)
          when :static then def_static_method(token)
          when :attribute then def_attribute(token)
          else
            raise ParseError, "A #{token.type.inspect} is not valid here"
          end

        nodes << node
      end

      AST::Body.new(nodes, start.location)
    end

    def implementation_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        node =
          case token.type
          when :define then def_method(token)
          when :move then def_move_method(token)
          when :static then def_static_method(token)
          else
            raise ParseError, "A #{token.type.inspect} is not valid here"
          end

        nodes << node
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses the definition of a trait.
    #
    # Examples:
    #
    #     trait Foo { ... }
    #     trait Foo!(T) { ... }
    #     trait Numeric: Add, Subtract { ... }
    def def_trait(start)
      name = advance_and_expect!(:constant)
      targs = optional_type_parameter_definitions

      required =
        if @lexer.next_type_is?(:colon)
          skip_one
          required_traits
        else
          []
        end

      body = trait_body(advance_and_expect!(:curly_open))

      AST::Trait.new(name.value, targs, required, body, start.location)
    end

    def trait_body(start)
      nodes = []

      while (token = @lexer.advance) && token.valid_but_not?(:curly_close)
        node =
          case token.type
          when :define then def_method(token)
          when :move then def_move_method(token)
          else
            raise ParseError, "A #{token.type.inspect} is not valid here"
          end

        nodes << node
      end

      AST::Body.new(nodes, start.location)
    end

    # Parses the implementation of a trait or re-opening of an object.
    #
    # Example:
    #
    #     impl ToString for Object {
    #       ...
    #     }
    def implement_trait(start)
      first_name = advance_and_expect!(:constant)

      if @lexer.next_type_is?(:type_args_open)
        implement_trait_for_object(type_name(first_name), start.location)
      elsif @lexer.next_type_is?(:for)
        implement_trait_for_object(
          AST::TypeName.new(
            constant_from_token(first_name),
            [],
            first_name.location
          ),
          start.location
        )
      else
        reopen_object(constant_from_token(first_name), start.location)
      end
    end

    def implement_trait_for_object(trait_name, location)
      advance_and_expect!(:for)

      object_name = constant_from_token(advance_and_expect!(:constant))
      body = implementation_body(advance_and_expect!(:curly_open))

      AST::TraitImplementation.new(
        trait_name,
        object_name,
        body,
        location
      )
    end

    def reopen_object(name, location)
      body = implementation_body(advance_and_expect!(:curly_open))

      AST::ReopenObject.new(name, body, location)
    end

    # Parses a return statement.
    #
    # Example:
    #
    #     return 10
    def return_value(start, location = start.location)
      value = expression(advance!) if next_expression_is_argument?(start)

      AST::Return.new(value, location)
    end

    def yield_value(start)
      value = expression(advance!) if next_expression_is_argument?(start)

      AST::Yield.new(value, start.location)
    end

    def attribute_or_reassign(start)
      return reassign_attribute(start) if @lexer.next_type_is?(:assign)

      node = attribute(start)

      if next_is_binary_reassignment?
        reassign_binary(node)
      else
        node
      end
    end

    # Parses an attribute.
    #
    # Examples:
    #
    #     @foo
    def attribute(start)
      attribute_from_token(start)
    end

    # Parses the re-assignment of a local variable.
    #
    # Example:
    #
    #     foo = 10
    def reassign_local(start)
      name = identifier_from_token(start)

      reassign_variable(name, start.location)
    end

    # Parses the re-assignment of an attribute.
    #
    # Example:
    #
    #     @foo = 10
    def reassign_attribute(start)
      name = attribute_from_token(start)

      reassign_variable(name, start.location)
    end

    # Parses the reassignment of a variable.
    #
    # Examples:
    #
    #     a = 10
    #     @a = 10
    def reassign_variable(name, location)
      advance_and_expect!(:assign)

      value = expression(advance!)

      AST::ReassignVariable.new(name, value, location)
    end

    # Parses a binary reassignment of a variable
    #
    # Examples:
    #
    #   a |= 10
    #   @a <<= 20
    def reassign_binary(variable)
      operator = advance!
      location = operator.location
      message = operator.value[0..-2]
      rhs = expression(advance!)
      value = AST::Send.new(message, variable, [], [rhs], location)

      AST::ReassignVariable.new(variable, value, location)
    end

    def next_is_binary_reassignment?
      BINARY_REASSIGN_OPERATORS.include?(@lexer.peek.type)
    end

    def self_object(start)
      AST::Self.new(start.location)
    end

    # Parses a "throw" statement.
    #
    # Example:
    #
    #     throw Foo
    def throw_value(start, location = start.location)
      AST::Throw.new(expression(advance!), location)
    end

    # Parses a "try" statement.
    #
    # Examples:
    #
    #     try foo
    #     try foo else bar
    #     try foo else (error) { error }
    def try(start, location = start.location)
      expression = try_expression
      else_arg = nil

      else_body =
        if @lexer.next_type_is?(:else)
          skip_one

          else_arg = optional_else_arg

          block_with_optional_curly_braces
        else
          AST::Body.new([], start.location)
        end

      AST::Try.new(expression, else_body, else_arg, location)
    end

    # Parses a "try!" statement
    def try_bang(start)
      expression = try_expression
      else_arg, else_body = try_bang_else(start)

      AST::Try.new(expression, else_body, else_arg, start.location)
    end

    def try_expression
      with_curly =
        if @lexer.next_type_is?(:curly_open)
          advance!
          true
        end

      expression = binary_send(advance!)

      advance_and_expect!(:curly_close) if with_curly

      expression
    end

    def try_bang_else(start)
      arg = try_bang_else_arg(start)
      loc = start.location

      body = [
        # _INKOC.panic(error.to_string)
        AST::Send.new(
          Config::PANIC_MESSAGE,
          AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, loc),
          [],
          [AST::Send.new(Config::TO_STRING_MESSAGE, arg, [], [], loc)],
          loc
        )
      ]

      [arg, AST::Body.new(body, loc)]
    end

    def try_bang_else_arg(start)
      AST::Identifier.new('error', start.location)
    end

    def block_with_optional_curly_braces
      if @lexer.next_type_is?(:curly_open)
        block_body(@lexer.advance)
      else
        start = advance!

        AST::Body.new([expression(start)], start.location)
      end
    end

    # Parses an optional argument for the "else" statement.
    def optional_else_arg
      return unless @lexer.next_type_is?(:paren_open)

      skip_one

      name = identifier_from_token(advance_and_expect!(:identifier))

      advance_and_expect!(:paren_close)

      name
    end

    def type(start)
      if start.type == :ref
        AST::ReferenceType.new(type_without_modifier(advance!), start.location)
      else
        type_without_modifier(start)
      end
    end

    def type_without_modifier(start)
      case start.type
      when :question
        type(advance!).tap { |t| t.optional = true }
      when :constant
        type_name(start)
      when :do, :lambda
        block_type(start, start.type)
      when :move
        moving_block_type(start)
      else
        raise(
          ParseError,
          "Unexpected #{start.type}, expected a constant or a ("
        )
      end
    end

    def pattern_match(start)
      bind_to = nil
      match_arms = []
      match_else = nil

      if @lexer.next_type_is?(:let)
        skip_one
        bind_to = identifier_from_token(advance_and_expect!(:identifier))
        advance_and_expect!(:assign)
      end

      to_match = without_trailing_block { expression(advance!) }

      advance_and_expect!(:curly_open)

      while @lexer.peek.valid_but_not?(:curly_close)
        token = @lexer.advance

        case token.type
        when :as
          type = constant(advance_and_expect!(:constant))
          guard =
            if @lexer.next_type_is?(:when)
              skip_one
              expression(advance!)
            end

          advance_and_expect!(:arrow)

          body = block_body(advance_and_expect!(:curly_open))

          match_arms << AST::MatchType.new(type, guard, body, token.location)
        when :else
          advance_and_expect!(:arrow)

          else_body = block_body(advance_and_expect!(:curly_open))
          match_else = AST::MatchElse.new(else_body, token.location)

          # "else" must be last
          break
        else
          tests = []

          tests << expression(token)

          while @lexer.next_type_is?(:comma)
            skip_one
            tests << expression(advance!)
          end

          guard =
            if @lexer.next_type_is?(:when)
              skip_one
              expression(advance!)
            end

          advance_and_expect!(:arrow)

          body = block_body(advance_and_expect!(:curly_open))

          match_arms << AST::MatchExpression.new(tests, guard, body, token.location)
        end
      end

      advance_and_expect!(:curly_close)

      AST::Match.new(to_match, bind_to, match_arms, match_else, start.location)
    end

    def if_expression(token)
      cond = without_trailing_block { expression(advance!) }
      body = block_body(advance_and_expect!(:curly_open))
      conds = [AST::IfCondition.new(cond, body, token.location)]
      else_body = nil

      while @lexer.next_type_is?(:else)
        else_start = advance!

        case @lexer.peek.type
        when :if
          advance!

          conds.push(
            AST::IfCondition.new(
              without_trailing_block { expression(advance!) },
              block_body(advance_and_expect!(:curly_open)),
              else_start.location
            )
          )
        when :curly_open
          else_body = block_body(advance_and_expect!(:curly_open))
          break
        else
          raise ParseError, "Unexpected #{@lexer.peek.type.inspect}"
        end
      end

      AST::If.new(conds, else_body, token.location)
    end

    def constant_from_token(token)
      AST::Constant.new(token.value, token.location)
    end

    def identifier_from_token(token, type_arguments = [])
      if type_arguments.any?
        AST::Send.new(token.value, nil, type_arguments, [], token.location)
      else
        AST::Identifier.new(token.value, token.location)
      end
    end

    def attribute_from_token(token)
      AST::Attribute.new(token.value, token.location)
    end

    def message_name_for_token(token)
      unless MESSAGE_TOKENS.include?(token.type)
        raise ParseError, "#{token.value.inspect} is not a valid message name"
      end

      name = token.value

      if token.type == :bracket_open
        advance_and_expect!(:bracket_close)

        name << ']'
      end

      if @lexer.next_type_is?(:assign)
        skip_one
        name << '='
      end

      name
    end

    def starting_location
      @starting_location ||= SourceLocation.new(1, 1, @lexer.file)
    end

    def skip_and_advance!
      @lexer.advance
      advance!
    end

    def skip_one
      @lexer.advance
    end

    def skip_one_if(type)
      skip_one if @lexer.peek.type == type
    end

    def advance!
      token = @lexer.advance

      raise(ParseError, 'Unexpected end of input') if token.nil?

      token
    end

    def advance_and_expect!(type)
      token = advance!

      return token if token.type == type

      raise(
        ParseError,
        "Expected a #{type.inspect}, got #{token.type.inspect} instead"
      )
    end

    def comma_or_break_on(break_on)
      token = @lexer.advance

      case token.type
      when :comma
        false
      when break_on
        true
      else
        raise(
          ParseError,
          "Unexpected #{token.type}, expected a comma or #{break_on}"
        )
      end
    end
  end
  # rubocop: enable Metrics/ClassLength
end
