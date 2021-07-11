# frozen_string_literal: true

module Inkoc
  module Pass
    class DesugarLoops
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(ast)
        [update_node(ast)]
      end

      def on_import(node)
        node
      end

      def on_body(node)
        update_nodes(node.expressions)
        node
      end

      def on_block(node)
        update_nodes(node.arguments)
        update_nodes(node.body.expressions)
        node
      end

      def on_lambda(node)
        update_nodes(node.arguments)
        update_nodes(node.body.expressions)
        node
      end

      def on_match(node)
        update_node(node.expression) if node.expression
        update_nodes(node.arms)
        update_node(node.match_else) if node.match_else
        node
      end

      def on_match_else(node)
        update_node(node.body)
        node
      end

      def on_match_type(node)
        update_nodes(node.body.expressions)
        update_node(node.guard) if node.guard

        node
      end

      def on_match_expression(node)
        update_nodes(node.body.expressions)
        update_nodes(node.patterns)
        update_node(node.guard) if node.guard

        node
      end

      def on_if(node)
        node.conditions.each do |cond|
          update_node(cond.condition)
          update_node(cond.body)
        end

        update_node(node.else_body) if node.else_body

        node
      end

      def on_and(node)
        update_node(node.left)
        update_node(node.right)
        node
      end

      def on_or(node)
        update_node(node.left)
        update_node(node.right)
        node
      end

      def on_not(node)
        update_node(node.expression)
        node
      end

      def on_method(node)
        update_nodes(node.arguments)
        update_nodes(node.body.expressions)
        node
      end

      def on_send(node)
        update_nodes(node.arguments)
        update_node(node.receiver) if node.receiver

        node
      end

      def on_raw_instruction(node)
        update_nodes(node.arguments)
        node
      end

      def on_node_with_body(node)
        update_node(node.body)
        node
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body
      alias on_required_method on_node_with_body

      def on_try(node)
        update_node(node.expression)
        update_node(node.else_body) if node.else_body

        node
      end

      def on_node_with_value(node)
        update_node(node.value) if node.value

        node
      end

      alias on_throw on_node_with_value
      alias on_return on_node_with_value
      alias on_define_variable on_node_with_value
      alias on_define_variable_with_explicit_type on_node_with_value
      alias on_reassign_variable on_node_with_value
      alias on_keyword_argument on_node_with_value
      alias on_destructure_array on_node_with_value

      def on_define_argument(node)
        node
      end

      def on_type_cast(node)
        update_node(node.expression)
        node
      end

      def on_new_instance(node)
        update_nodes(node.attributes)
        node
      end

      def on_assign_attribute(node)
        update_node(node.value)
        node
      end

      def on_template_string(node)
        update_nodes(node.members)
        node
      end

      def on_group(node)
        update_nodes(node.expressions)
        node
      end

      def on_for(node)
        process_node(node.body)
        process_node(node.else_body) if node.else_body

        case node.binding
        when AST::ForBinding
          desugar_for_with_binding(node)
        when AST::ForDestructure
          desugar_for_with_destructure(node)
        end
      end

      def define_for_loop_iterator(node, location)
        iter_var_name = "__iter#{node.object_id}"

        # let __iterX = _INKOC.to_iterator(iterable)
        AST::DefineVariable.new(
          AST::Identifier.new(iter_var_name, location),
          AST::Send.new(
            'to_iterator',
            AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, location),
            [],
            [node.iterable],
            location
          ),
          nil,
          false,
          location
        )
      end

      def for_loop_next_variable(for_node, iterator, name, value_type, mutable)
        iterable = for_node.iterable
        location = iterable.location

        get_next = AST::Send.new(
          Config::NEXT_MESSAGE,
          AST::Identifier.new(iterator, location),
          [],
          [],
          location
        )

        get_next =
          case for_node.error
          when :try
            for_loop_try(for_node, get_next, location)
          when :try_bang
            for_loop_try_bang(for_node, get_next, location)
          else
            get_next
          end

        next_var_name = "__iter_next#{iterable.object_id}"

        # let __itex_nextX = iter.next
        let_next_var = AST::DefineVariable.new(
          AST::Identifier.new(next_var_name, location),
          get_next,
          nil,
          false,
          location
        )

        # let var = try __iter_nextX.get else break
        let_var = AST::DefineVariable.new(
          AST::Identifier.new(name, location),
          AST::Try.new(
            AST::Send.new(
              Config::GET_MESSAGE,
              AST::Identifier.new(next_var_name, location),
              [],
              [],
              location
            ),
            AST::Body.new([AST::Break.new(location)], location),
            nil,
            location
          ),
          value_type,
          mutable,
          location
        )

        AST::Group.new([let_next_var, let_var], location)
      end

      def for_loop_try(for_node, get_next, location)
        else_body =
          if for_node.else_body
            AST::Body.new(
              [
                *for_node.else_body.expressions,
                AST::Break.new(location)
              ],
              location
            )
          else
            AST::Body.new([], location)
          end

        AST::Try.new(
          get_next,
          else_body,
          for_node.else_argument,
          location
        )
      end

      def for_loop_try_bang(for_node, get_next, location)
        err = AST::Identifier.new('error', location)

        AST::Try.new(
          get_next,
          AST::Body.new(
            [
              AST::Send.new(
                Config::PANIC_MESSAGE,
                AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, location),
                [],
                [
                  AST::Send.new(Config::TO_STRING_MESSAGE, err, [], [], location)
                ],
                location
              )
            ],
            location
          ),
          err,
          location
        )
      end

      def desugar_for_with_binding(node)
        let_iter_var = define_for_loop_iterator(node, node.iterable.location)
        next_var_name = node.binding.name.name
        iter_var_name = let_iter_var.variable.name
        let_next_var = for_loop_next_variable(
          node,
          iter_var_name,
          next_var_name,
          node.binding.value_type,
          node.binding.mutable
        )

        loop_node = AST::Loop.new(
          AST::Body.new([let_next_var, *node.body.expressions], node.location),
          node.location
        )

        AST::Group.new([let_iter_var, loop_node], node.location)
      end

      def desugar_for_with_destructure(node)
        var_loc = node.binding.location
        let_iter_var = define_for_loop_iterator(node, var_loc)
        temp_var_name = "__value#{node.object_id}"
        iter_var_name = let_iter_var.variable.name
        let_temp_var = for_loop_next_variable(
          node,
          iter_var_name,
          temp_var_name,
          nil,
          false
        )

        let_destr_var = AST::DestructArray.new(
          node.binding.variables,
          AST::Identifier.new(temp_var_name, var_loc),
          var_loc
        )

        loop_node = AST::Loop.new(
          AST::Body.new(
            [let_temp_var, let_destr_var, *node.body.expressions],
            node.location
          ),
          node.location
        )

        AST::Group.new([let_iter_var, loop_node], node.location)
      end

      def on_while(node)
        process_node(node.body)

        cond_loc = node.condition.location
        if_cond = AST::If.new(
          [
            # if not condition { break }
            AST::IfCondition.new(
              AST::Not.new(node.condition, cond_loc),
              AST::Body.new([AST::Break.new(cond_loc)], cond_loc),
              cond_loc
            )
          ],
          nil,
          cond_loc
        )

        loop_body = AST::Body
          .new([if_cond, *node.body.expressions], node.body.location)

        AST::Loop.new(loop_body, node.location)
      end
    end
  end
end
